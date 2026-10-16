//! Workspace: locked directory with plugin clones, deployed `dist/` and `state.json`.
//! Every modification stages only the paths it touches and commits them through a
//! journal (see `txn`), so a crash at any point leaves a consistent workspace.
use crate::error::{Context, Error, Require, Result, ensure, fail};
use crate::{
    ForkOf, Installed, InstalledPlugin, LostWork, Manifest, Replaces, Revision, State, catalog,
    check::{self, CheckRow},
    deps,
    exec::git,
    identity,
    lost_work::{self, clean},
    model::{manifest, manifest_json, resolve_ref, resolve_upstream_ref, revision},
    releases, scaffold,
    txn::{self, Txn},
    valid_id,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

/// Settings passed explicitly by the caller; the library never reads environment variables.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Catalog location (local folder, JSON file or HTTP(S) URL) for catalog-based operations.
    pub catalog: Option<String>,
}

/// Result of an install.
#[derive(Clone, Debug, Serialize)]
pub struct InstallResult {
    pub id: String,
    pub source: String,
    pub current: Revision,
    #[serde(rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
    /// Broken release tags passed over while choosing the latest release (direct
    /// installs without an explicit ref); the caller should warn about them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<releases::SkippedTag>,
}
/// Result of update/rollback: `from` → `to`; `changed` is false for a no-op update.
#[derive(Clone, Debug, Serialize)]
pub struct RevisionChange {
    pub id: String,
    pub from: Revision,
    pub to: Revision,
    pub changed: bool,
}
/// What `uninstall` would delete and which local work would be lost.
#[derive(Clone, Debug, Serialize)]
pub struct UninstallPlan {
    pub id: String,
    /// Registered in state (false for a forgotten, retained repository).
    pub registered: bool,
    /// Absolute paths that would be removed.
    pub deletes: Vec<String>,
    /// Empty for a clean clone: then uninstall needs no confirmation.
    pub lost_work: Vec<LostWork>,
    /// Installed plugins that depend on this one: uninstall refuses unless forced.
    pub dependents: Vec<String>,
}

pub struct Manager {
    root: PathBuf,
    options: Options,
    /// Serializes operations of this handle (stage/backup paths are shared).
    busy: Mutex<()>,
    _lock: crate::lock::FileLock,
}

/// Copy a tree. With `symlinks` they are recreated as symlinks (a repository may
/// track them); without, any symlink is refused (deployed `dist/` must be plain files).
pub(crate) fn copy_tree(from: &Path, to: &Path, symlinks: bool) -> Result<()> {
    let meta =
        fs::symlink_metadata(from).context(Error::Io, format!("missing {}", from.display()))?;
    if meta.file_type().is_symlink() {
        ensure!(
            symlinks,
            Unsupported,
            "symlinks are not supported: {}",
            from.display()
        );
        std::os::unix::fs::symlink(fs::read_link(from)?, to)?;
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()), symlinks)?;
        }
    } else {
        ensure!(
            meta.is_file(),
            Unsupported,
            "unsupported file: {}",
            from.display()
        );
        fs::copy(from, to)?;
    }
    Ok(())
}
/// Each component of `id` exists under `base` with exactly this spelling. On a
/// case-insensitive file system `Vendor/Module` must not resolve to `vendor/module`.
fn exact_path(base: &Path, id: &str) -> Result<bool> {
    let mut current = base.to_path_buf();
    for part in id.split('/') {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let mut found = false;
        for entry in entries {
            if entry?.file_name() == part {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
        current.push(part);
    }
    Ok(true)
}
/// Refuse an ID that would land on an existing path spelled differently (`Dan/p` vs
/// `dan/p` on a case-insensitive file system): it would share or overwrite that clone.
fn ensure_no_case_collision(base: &Path, id: &str) -> Result<()> {
    let mut current = base.to_path_buf();
    for part in id.split('/') {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let name = entry?.file_name();
            if name != part && name.to_string_lossy().eq_ignore_ascii_case(part) {
                return Err(Error::Workspace(format!(
                    "plugin ID {id} collides with an existing path that differs only in letter case"
                )));
            }
        }
        current.push(part);
    }
    Ok(())
}
fn ids_collide(a: &str, b: &str) -> bool {
    let (a, b): (Vec<_>, Vec<_>) = (a.split('/').collect(), b.split('/').collect());
    a.iter()
        .zip(&b)
        .take_while(|(x, y)| x.eq_ignore_ascii_case(y))
        .any(|(x, y)| x != y)
}
fn plugin_repo(root: &Path, id: &str) -> Result<PathBuf> {
    let plugins = root.join("plugins");
    let repo = plugins.join(id);
    if !exact_path(&plugins, id)? || !repo.join(".git").is_dir() {
        return Err(Error::RepositoryNotFound { id: id.into() });
    }
    Ok(repo)
}

impl Manager {
    /// A dedicated workspace only: never point this at the application's live data.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(root, Options::default())
    }
    pub fn open_with(root: impl AsRef<Path>, options: Options) -> Result<Self> {
        let root = std::path::absolute(root)?;
        let file_name = root
            .file_name()
            .require(Error::Workspace, "workspace needs a directory name")?;
        let parent = root
            .parent()
            .require(Error::Workspace, "workspace needs a parent")?;
        fs::create_dir_all(parent)?;
        // Keyed by the real path: a symlinked spelling of the root or of its parent is
        // the same workspace and must take the same lock.
        let real = if fs::symlink_metadata(&root).is_ok() {
            fs::canonicalize(&root)?
        } else {
            fs::canonicalize(parent)?.join(file_name)
        };
        let name = real
            .file_name()
            .require(Error::Workspace, "workspace needs a directory name")?
            .to_str()
            .require(Error::Workspace, "non-UTF8 workspace name")?;
        let lock = crate::lock::try_lock(
            &real
                .parent()
                .require(Error::Workspace, "workspace needs a parent")?
                .join(format!(".{name}.lock")),
            "workspace is already in use",
        )?;
        let manager = Self {
            root: root.clone(),
            options,
            busy: Mutex::new(()),
            _lock: lock,
        };
        if root.join("state.json").exists() {
            txn::recover(&root)?;
        }
        if root.exists() && !root.join("state.json").exists() {
            ensure!(
                fs::read_dir(&root)?.next().is_none(),
                Workspace,
                "refusing nonempty directory without state.json"
            );
        }
        fs::create_dir_all(root.join("plugins"))?;
        fs::create_dir_all(root.join("dist"))?;
        if !root.join("state.json").exists() {
            fs::write(
                root.join("state.json"),
                serde_json::to_vec_pretty(&State::default())?,
            )?;
        }
        manager.state()?;
        Ok(manager)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn options(&self) -> &Options {
        &self.options
    }
    fn busy(&self) -> MutexGuard<'_, ()> {
        self.busy.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn catalog_location(&self) -> Result<&str> {
        self.options
            .catalog
            .as_deref()
            .require(Error::InvalidArgument, "no catalog configured")
    }
    pub fn state(&self) -> Result<State> {
        let state: State = serde_json::from_slice(&fs::read(self.root.join("state.json"))?)?;
        for id in state.plugins.keys() {
            valid_id(id)?;
        }
        Ok(state)
    }
    /// Installed plugins as a flat list (sorted by ID).
    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        Ok(self.state()?.installed())
    }
    /// Run `action` on a transaction that stages only the paths it touches; nothing
    /// in the live workspace changes unless the action succeeds.
    fn transaction<T>(&self, action: impl FnOnce(&mut Txn, &mut State) -> Result<T>) -> Result<T> {
        let _busy = self.busy();
        let mut txn = Txn::begin(&self.root)?;
        let mut state = self.state()?;
        let value = action(&mut txn, &mut state)?;
        txn.commit(&state)?;
        Ok(value)
    }

    /// Compare installed versions with the configured catalog. Read-only.
    pub fn check(&self) -> Result<Vec<CheckRow>> {
        let state = self.state()?;
        if state.plugins.is_empty() {
            return Ok(Vec::new());
        }
        let location = self.catalog_location()?;
        check::check_state(&state, location)
    }
    /// Bounded overview of the configured catalog.
    pub fn browse_catalog(&self) -> Result<catalog::CatalogOverview> {
        catalog::browse(self.catalog_location()?)
    }
    /// Exact catalog release (latest if `version` is `None`) from the configured catalog.
    pub fn catalog_release(&self, id: &str, version: Option<&str>) -> Result<catalog::Release> {
        catalog::lookup(self.catalog_location()?, id, version)
    }

    /// Copy the installed repository, including local edits, into an independent author folder.
    pub fn fork(&self, id: &str, destination: &Path) -> Result<PathBuf> {
        self.fork_in_namespace(id, destination, identity::DEFAULT_NAMESPACE)
    }
    pub fn fork_in_namespace(
        &self,
        id: &str,
        destination: &Path,
        namespace: &str,
    ) -> Result<PathBuf> {
        valid_id(id)?;
        let _busy = self.busy();
        let state = self.state()?;
        let installed = state
            .plugins
            .get(id)
            .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
        let new_id = identity::qualified(namespace, id.rsplit('/').next().unwrap())?;
        ensure!(
            new_id != id,
            InvalidArgument,
            "fork needs a different namespace; use --namespace NAME"
        );
        ensure!(
            !ids_collide(&new_id, id)
                && !state
                    .plugins
                    .keys()
                    .any(|other| ids_collide(other, &new_id)),
            InvalidArgument,
            "fork ID {new_id} differs only in letter case from an installed plugin"
        );
        let source = self.root.join("plugins").join(id);
        ensure!(
            source.join(".git").is_dir(),
            Workspace,
            "expected a standalone plugin git repository"
        );
        ensure!(
            !destination
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            InvalidArgument,
            "use a destination without '..'"
        );
        let destination = std::path::absolute(destination)?;
        ensure!(
            !destination.try_exists()? && fs::symlink_metadata(&destination).is_err(),
            InvalidArgument,
            "destination already exists"
        );
        let root = fs::canonicalize(&self.root)?;
        let ancestor = destination.ancestors().find(|p| p.exists()).require(
            Error::InvalidArgument,
            "destination has no existing ancestor",
        )?;
        ensure!(
            !fs::canonicalize(ancestor)?.starts_with(&root),
            InvalidArgument,
            "fork destination must be outside the managed workspace"
        );
        let parent = destination
            .parent()
            .require(Error::InvalidArgument, "destination needs a parent")?;
        fs::create_dir_all(parent)?;
        fs::create_dir(&destination)?;
        let result = (|| {
            copy_tree(&source, &destination, true)?;
            let remotes = git(&destination, &["remote"])?;
            if remotes.lines().any(|r| r == "origin") {
                git(&destination, &["remote", "remove", "origin"])?;
            }
            if remotes.lines().any(|r| r == "upstream") {
                git(&destination, &["remote", "remove", "upstream"])?;
            }
            git(
                &destination,
                &["remote", "add", "--", "upstream", &installed.source],
            )?;
            let mut branch = "fork".to_owned();
            let mut number = 2;
            while !git(&destination, &["branch", "--list", &branch])?.is_empty() {
                branch = format!("fork-{number}");
                number += 1;
            }
            git(&destination, &["checkout", "-b", &branch])?;
            // The lineage belongs to this author checkout, not to installed state or the catalog.
            git(
                &destination,
                &[
                    "config",
                    "--local",
                    "pm.forkBase",
                    &installed.current.commit,
                ],
            )?;
            git(
                &destination,
                &[
                    "config",
                    "--local",
                    "pm.forkVersion",
                    &installed.current.version,
                ],
            )?;
            let base_commit = git(&destination, &["rev-parse", "HEAD"])?;
            let base: Manifest =
                serde_json::from_str(&git(&destination, &["show", "HEAD:plugin.json"])?)?;
            ensure!(
                base.id == id,
                InvalidManifest,
                "installed repository HEAD has a different plugin ID"
            );
            let mut m = manifest(&destination)?;
            m.id = new_id.clone();
            // The fork can stand in for the original at the version it started from;
            // this is used only if the user explicitly replaces the original.
            m.replaces = Some(Replaces {
                id: id.into(),
                version: base.version.clone(),
            });
            m.fork_of = Some(ForkOf {
                id: id.into(),
                version: base.version,
                commit: base_commit.clone(),
            });
            fs::write(destination.join("plugin.json"), manifest_json(&m)?)?;
            scaffold::ensure_identity(&destination)?;
            // Commit only the identity change; keep unrelated local edits and staged files intact.
            git(
                &destination,
                &[
                    "commit",
                    "--only",
                    "-m",
                    &format!("Fork {id} as {new_id}"),
                    "--",
                    "plugin.json",
                ],
            )?;
            // No tag yet: local edits must be explicitly committed into a release.
            git(
                &destination,
                &["config", "--local", "pm.forkBase", &base_commit],
            )?;
            git(
                &destination,
                &[
                    "config",
                    "--local",
                    "pm.forkVersion",
                    &m.fork_of.as_ref().unwrap().version,
                ],
            )?;
            Ok(fs::canonicalize(&destination)?)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&destination);
        }
        result
    }

    /// Install the latest release tag of a Git source.
    pub fn install(&self, source: &str) -> Result<InstallResult> {
        self.install_ref(source, None, None)
    }
    /// Install a Git ref (tag, branch, commit). `expected` must match the manifest exactly.
    pub fn install_ref(
        &self,
        source: &str,
        reference: Option<&str>,
        expected: Option<&Manifest>,
    ) -> Result<InstallResult> {
        self.install_checked(
            Fetch {
                source,
                reference,
                expected,
                files: None,
            },
            None,
        )
    }
    /// Stable channel installs the latest release tag (or `reference`); dev follows HEAD.
    pub fn install_channel(
        &self,
        source: &str,
        reference: Option<&str>,
        dev: bool,
    ) -> Result<InstallResult> {
        ensure!(
            !dev || reference.is_none(),
            InvalidArgument,
            "--channel dev conflicts with --ref"
        );
        self.install_ref(source, if dev { Some("HEAD") } else { reference }, None)
    }
    /// Install an exact catalog release; manifest and dist hashes are verified.
    /// Missing dependencies come from the configured catalog.
    pub fn install_release(&self, release: &catalog::Release) -> Result<InstallResult> {
        self.install_release_from(release, None)
    }
    fn install_release_from(
        &self,
        release: &catalog::Release,
        origin: Option<String>,
    ) -> Result<InstallResult> {
        catalog::validate_release(release, &release.manifest.id, &release.manifest.version)?;
        self.install_checked(Fetch::release(release), origin)
    }
    /// Look up `id` (latest if `version` is `None`) in the configured catalog and install
    /// it with its dependencies. The workspace works with one catalog: installing from
    /// another one than earlier catalog installs is refused.
    pub fn install_from_catalog(&self, id: &str, version: Option<&str>) -> Result<InstallResult> {
        let origin = catalog::canonical_location(self.catalog_location()?)?;
        ensure_single_catalog(&self.state()?, &origin)?;
        let release = self.catalog_release(id, version)?;
        self.install_release_from(&release, Some(origin))
    }
    fn install_checked(&self, fetch: Fetch, origin: Option<String>) -> Result<InstallResult> {
        ensure!(
            !fetch.source.starts_with('-'),
            InvalidArgument,
            "invalid git source"
        );
        let source = if Path::new(fetch.source).exists() {
            fs::canonicalize(fetch.source)?
                .to_string_lossy()
                .into_owned()
        } else {
            fetch.source.to_owned()
        };
        self.transaction(|txn, state| {
            let fetch = Fetch {
                source: &source,
                ..fetch
            };
            let (repo, m, skipped) = clone_revision(txn, &fetch, "incoming")?;
            check_new_id(txn.root(), state, &m)?;
            // Everything installed stays at its version; the new plugin is added on top.
            let mut next = state.clone();
            next.plugins.insert(m.id.clone(), Installed::default());
            next.plugins.get_mut(&m.id).unwrap().current.version = m.version.clone();
            next.plugins.get_mut(&m.id).unwrap().adopt(&m);
            let mut roots = deps::healthy_roots(state)?;
            if !state.replacements.contains_key(&m.id) {
                roots.insert(m.id.clone(), deps::version(&m.version)?);
            }
            let mut fixed = deps::fixed_from_state(&next)?;
            if let Some(new) = fixed.get_mut(&m.id) {
                new.note = Some(format!("being installed at {}", m.version));
            }
            let missing = deps::solve(&fixed, &roots, self.options.catalog.as_deref())?;
            self.install_missing(txn, state, &missing)?;
            let mut result = adopt(txn, state, &repo, &m, source.clone(), origin.clone())?;
            result.skipped = skipped;
            Ok(result)
        })
    }
    /// Install catalog releases chosen by the solver (inside the current transaction).
    fn install_missing(
        &self,
        txn: &mut Txn,
        state: &mut State,
        missing: &BTreeMap<String, semver::Version>,
    ) -> Result<()> {
        if missing.is_empty() {
            return Ok(());
        }
        let location = self.catalog_location()?;
        let origin = catalog::canonical_location(location)?;
        ensure_single_catalog(state, &origin)?;
        for (number, (id, version)) in missing.iter().enumerate() {
            let release = catalog::lookup(location, id, Some(&version.to_string()))?;
            catalog::validate_release(&release, id, &release.manifest.version)?;
            let (repo, m, _) = clone_revision(
                txn,
                &Fetch::release(&release),
                &format!("dependency-{number}"),
            )?;
            check_new_id(txn.root(), state, &m)?;
            adopt(
                txn,
                state,
                &repo,
                &m,
                release.source.clone(),
                Some(origin.clone()),
            )?;
        }
        Ok(())
    }
    /// Check that `id` moving to manifest `m` keeps every working dependency working;
    /// with `fetch`, dependencies the new revision adds are installed from the catalog.
    fn check_change(
        &self,
        txn: &mut Txn,
        state: &mut State,
        id: &str,
        m: &Manifest,
        fetch: bool,
    ) -> Result<()> {
        for (original, fork) in &state.replacements {
            ensure!(
                fork != id || m.replaces.as_ref().is_some_and(|r| r.id == *original),
                VersionConflict,
                "{id} {} no longer declares that it replaces {original}; switch {original} back first",
                m.version
            );
        }
        let mut next = state.clone();
        let entry = next
            .plugins
            .get_mut(id)
            .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
        entry.current.version = m.version.clone();
        entry.adopt(m);
        let mut roots = BTreeMap::new();
        for root in deps::healthy_roots(state)?
            .into_keys()
            .chain([id.to_owned()])
        {
            if !next.replacements.contains_key(&root) {
                roots.insert(
                    root.clone(),
                    deps::version(&next.plugins[&root].current.version)?,
                );
            }
        }
        let mut fixed = deps::fixed_from_state(&next)?;
        if let Some(changed) = fixed.get_mut(id) {
            changed.note = Some(format!("being changed to {}", m.version));
        }
        let missing = deps::solve(
            &fixed,
            &roots,
            if fetch {
                self.options.catalog.as_deref()
            } else {
                None
            },
        )?;
        self.install_missing(txn, state, &missing)
    }
    pub fn update(&self, id: &str) -> Result<RevisionChange> {
        self.update_ref(id, None)
    }
    pub fn update_ref(&self, id: &str, reference: Option<&str>) -> Result<RevisionChange> {
        self.update_channel(id, reference, false)
    }
    /// Update to `reference`, the latest release tag (stable) or origin HEAD (dev).
    /// Refused if the new version breaks a dependent; new dependencies are installed.
    pub fn update_channel(
        &self,
        id: &str,
        reference: Option<&str>,
        dev: bool,
    ) -> Result<RevisionChange> {
        ensure!(
            !dev || reference.is_none(),
            InvalidArgument,
            "--channel dev conflicts with --ref"
        );
        valid_id(id)?;
        self.transaction(|txn, state| {
            let p = state
                .plugins
                .get(id)
                .cloned()
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
            let live = plugin_repo(txn.root(), id)?;
            clean(&live, id)?;
            lost_work::require_head(
                &live,
                id,
                &p.current.commit,
                "local commits detected; update would discard them",
            )?;
            lost_work::require_no_hidden_for_update(
                &live,
                id,
                "local branches, tags or stash hold unpublished work; keep or remove them before updating",
            )?;
            let repo = txn.repo(id, &live)?;
            let known_releases = match releases::discover(&repo, Some(id)) {
                Ok(tags) => tags,
                Err(Error::NoReleases(_)) => Vec::new(),
                Err(error) => return Err(error),
            };
            // Mirror upstream tags exactly: a deleted release disappears, a moved one is
            // seen at its new commit (and refused below as a version conflict). Local
            // tags holding unpublished commits were refused above, so nothing is lost.
            git(
                &repo,
                &["fetch", "origin", "--prune", "--prune-tags", "--force", "--tags"],
            )?;
            for known in known_releases {
                let tag = format!("refs/tags/{}", known.name);
                // Deleting a tag is allowed; moving an observed release is not.
                if git(&repo, &["tag", "--list", &known.name])? == known.name {
                    ensure!(
                        resolve_ref(&repo, &tag)? == known.commit,
                        VersionConflict,
                        "{}",
                        if known.manifest.version == p.current.version {
                            "release commit changed for installed version".to_owned()
                        } else {
                            format!("release tag {} changed commit", known.name)
                        }
                    );
                }
            }
            if dev || reference == Some("HEAD") {
                git(&repo, &["remote", "set-head", "origin", "--auto"])?;
            }
            let target = if let Some(reference) = reference {
                resolve_upstream_ref(&repo, reference)?
            } else if dev {
                resolve_ref(&repo, "refs/remotes/origin/HEAD")?
            } else {
                let latest = releases::latest(&repo, Some(id))?;
                let latest_version = catalog::version_key(&latest.manifest.version)?;
                let current_version = catalog::version_key(&p.current.version)?;
                ensure!(
                    latest_version >= current_version,
                    VersionConflict,
                    "latest release is older than installed version; use an explicit --ref to downgrade"
                );
                ensure!(
                    latest_version != current_version || latest.commit == p.current.commit,
                    VersionConflict,
                    "release commit changed for installed version"
                );
                latest.commit
            };
            if target == p.current.commit {
                return Ok(RevisionChange {
                    id: id.into(),
                    from: p.current.clone(),
                    to: p.current.clone(),
                    changed: false,
                });
            }
            if dev {
                git(
                    &repo,
                    &["merge-base", "--is-ancestor", &p.current.commit, &target],
                )
                .context(
                    Error::VersionConflict,
                    "upstream history diverged; automatic dev update refused",
                )?;
            }
            git(&repo, &["checkout", "--detach", &target])?;
            let m = manifest(&repo)?;
            ensure!(m.id == id, InvalidManifest, "plugin id changed upstream");
            // Dev identity is the commit; release channels require a version bump.
            ensure!(
                dev || m.version != p.current.version,
                VersionConflict,
                "upstream changed without a version bump"
            );
            self.check_change(txn, state, id, &m, true)?;
            let next = revision(&repo, &m)?;
            let entry = state.plugins.get_mut(id).unwrap();
            entry.previous = Some(p.current.clone());
            entry.current = next.clone();
            entry.adopt(&m);
            txn.deploy(id)?;
            Ok(RevisionChange {
                id: id.into(),
                from: p.current,
                to: next,
                changed: true,
            })
        })
    }
    /// Swap current and previous version (offline). Refused if it breaks a dependent.
    pub fn rollback(&self, id: &str) -> Result<RevisionChange> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            let p = state
                .plugins
                .get(id)
                .cloned()
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
            let previous = p
                .previous
                .clone()
                .ok_or_else(|| Error::NoPreviousVersion { id: id.into() })?;
            let live = plugin_repo(txn.root(), id)?;
            clean(&live, id)?;
            lost_work::require_head(&live, id, &p.current.commit, "local commits detected")?;
            lost_work::require_no_hidden(
                &live,
                id,
                "local branches, tags or stash hold unpublished work; keep or remove them before rolling back",
            )?;
            let repo = txn.repo(id, &live)?;
            git(&repo, &["checkout", "--detach", &previous.commit])?;
            let m = manifest(&repo)?;
            ensure!(
                m.id == id && m.version == previous.version,
                InvalidManifest,
                "rollback manifest mismatch"
            );
            self.check_change(txn, state, id, &m, false)?;
            let entry = state.plugins.get_mut(id).unwrap();
            entry.previous = Some(p.current.clone());
            entry.current = previous.clone();
            entry.adopt(&m);
            txn.deploy(id)?;
            Ok(RevisionChange {
                id: id.into(),
                from: p.current,
                to: previous,
                changed: true,
            })
        })
    }
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        self.set_enabled_with(id, enabled, false)
    }
    /// Disabling a plugin others depend on is refused with [`Error::HasDependents`]
    /// unless `force`.
    pub fn set_enabled_with(&self, id: &str, enabled: bool, force: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|_, state| {
            if !state.plugins.contains_key(id) {
                return Err(Error::NotInstalled { id: id.into() });
            }
            if !enabled && !force {
                refuse_dependents(state, id, "disable")?;
            }
            state.plugins.get_mut(id).unwrap().enabled = enabled;
            Ok(())
        })
    }
    pub fn enable(&self, id: &str) -> Result<()> {
        self.set_enabled(id, true)
    }
    pub fn disable(&self, id: &str) -> Result<()> {
        self.set_enabled(id, false)
    }
    /// Forget registration but retain both repository and deployed files.
    pub fn forget(&self, id: &str) -> Result<()> {
        self.forget_with(id, false)
    }
    /// Like [`Manager::forget`]; dependents refuse unless `force`. Replacements this
    /// plugin provides are dropped (their consumers go back to the original).
    pub fn forget_with(&self, id: &str, force: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|_, state| {
            if !state.plugins.contains_key(id) {
                return Err(Error::NotInstalled { id: id.into() });
            }
            if !force {
                refuse_dependents(state, id, "forget")?;
            }
            state.plugins.remove(id);
            state.replacements.retain(|_, fork| fork != id);
            Ok(())
        })
    }
    /// Re-register a retained (forgotten) clean repository; previous history is not restored.
    pub fn restore(&self, id: &str) -> Result<InstallResult> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            // A missing clone is the real problem even when the ID is still registered.
            let repo = plugin_repo(txn.root(), id)?;
            if state.plugins.contains_key(id) {
                return Err(Error::AlreadyInstalled {
                    id: id.into(),
                    message: format!("plugin {id} is already registered"),
                });
            }
            clean(&repo, id)?;
            let m = manifest(&repo)?;
            ensure!(m.id == id, InvalidManifest, "plugin id mismatch");
            let mut installed = Installed {
                source: git(&repo, &["remote", "get-url", "origin"])?,
                current: revision(&repo, &m)?,
                previous: None,
                enabled: true,
                ..Default::default()
            };
            installed.adopt(&m);
            let result = InstallResult {
                id: id.into(),
                source: installed.source.clone(),
                current: installed.current.clone(),
                fork_of: installed.fork_of.clone(),
                skipped: Vec::new(),
            };
            state.plugins.insert(id.to_owned(), installed);
            txn.deploy(id)?;
            Ok(result)
        })
    }
    /// Read-only: what `uninstall` would delete, which local work would be lost and
    /// which installed plugins depend on it.
    /// Matches the exact ID spelling (never a case variant on a case-insensitive FS).
    pub fn uninstall_plan(&self, id: &str) -> Result<UninstallPlan> {
        valid_id(id)?;
        let _busy = self.busy();
        let state = self.state()?;
        let repo = plugin_repo(&self.root, id)?;
        let recorded = state.plugins.get(id).map(|p| p.current.commit.as_str());
        let lost_work = lost_work::detect(&repo, recorded)?;
        let mut deletes = vec![repo.to_string_lossy().into_owned()];
        let dist = self.root.join("dist").join(id);
        if dist.exists() {
            deletes.push(dist.to_string_lossy().into_owned());
        }
        Ok(UninstallPlan {
            id: id.into(),
            registered: state.plugins.contains_key(id),
            deletes,
            lost_work,
            dependents: deps::dependents(&state, id),
        })
    }
    /// Remove state entry, repository and dist. With `confirm == false` any local work
    /// (changes, untracked/ignored files, unpublished commits/branches/tags, stash)
    /// refuses with [`Error::LocalChanges`]; `confirm == true` deletes it anyway.
    /// Dependents refuse with [`Error::HasDependents`]; see [`Manager::uninstall_with`].
    pub fn uninstall(&self, id: &str, confirm: bool) -> Result<()> {
        self.uninstall_with(id, confirm, false)
    }
    /// [`Manager::uninstall`] that removes a plugin others depend on when `force`; the
    /// dependents then report an unsatisfied dependency.
    pub fn uninstall_with(&self, id: &str, confirm: bool, force: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            let repo = plugin_repo(txn.root(), id)?;
            if !force {
                refuse_dependents(state, id, "uninstall")?;
            }
            if !confirm {
                let recorded = state.plugins.get(id).map(|p| p.current.commit.as_str());
                let items = lost_work::detect(&repo, recorded)?;
                if !items.is_empty() {
                    return Err(lost_work::refusal(id, items));
                }
            }
            txn.delete(id)?;
            state.plugins.remove(id);
            state.replacements.retain(|_, fork| fork != id);
            Ok(())
        })
    }

    /// Read-only: what replacing `original` by `replacement` for every consumer in the
    /// workspace would do. Compatibility is by version only: each consumer's
    /// requirement on `original` against the version `replacement` declares in
    /// `replaces`.
    pub fn replace_plan(&self, original: &str, replacement: &str) -> Result<ReplacementPlan> {
        let _busy = self.busy();
        replacement_plan(&self.state()?, original, Some(replacement))
    }
    /// Make `replacement` (an installed plugin declaring `replaces: original`) the
    /// provider of `original` for every consumer. Unsatisfied consumers refuse with
    /// [`Error::IncompatibleReplacement`] unless `confirm`. Self and cyclic replacement
    /// are always refused. Replacing again switches the single binding.
    pub fn replace(
        &self,
        original: &str,
        replacement: &str,
        confirm: bool,
    ) -> Result<ReplacementPlan> {
        self.transaction(|_, state| {
            let plan = replacement_plan(state, original, Some(replacement))?;
            plan.refuse_unless(confirm)?;
            state
                .replacements
                .insert(original.into(), replacement.into());
            Ok(plan)
        })
    }
    /// Read-only plan for [`Manager::switch_back`].
    pub fn switch_back_plan(&self, original: &str) -> Result<ReplacementPlan> {
        let _busy = self.busy();
        replacement_plan(&self.state()?, original, None)
    }
    /// Drop the replacement of `original`: every consumer uses the installed original
    /// again. The original must be installed; unsatisfied consumers refuse unless
    /// `confirm`. The fork stays installed.
    pub fn switch_back(&self, original: &str, confirm: bool) -> Result<ReplacementPlan> {
        self.transaction(|_, state| {
            let plan = replacement_plan(state, original, None)?;
            plan.refuse_unless(confirm)?;
            state.replacements.remove(original);
            Ok(plan)
        })
    }
}

/// Where an install takes its revision from.
#[derive(Clone, Copy)]
struct Fetch<'a> {
    source: &'a str,
    reference: Option<&'a str>,
    expected: Option<&'a Manifest>,
    files: Option<&'a BTreeMap<String, catalog::FileDigest>>,
}
impl<'a> Fetch<'a> {
    fn release(release: &'a catalog::Release) -> Self {
        Fetch {
            source: &release.source,
            reference: Some(&release.commit),
            expected: Some(&release.manifest),
            files: (!release.files.is_empty()).then_some(&release.files),
        }
    }
}

/// Clone into the transaction's scratch area, check out and verify the revision.
fn clone_revision(
    txn: &Txn,
    fetch: &Fetch,
    name: &str,
) -> Result<(PathBuf, Manifest, Vec<releases::SkippedTag>)> {
    let repo = txn.scratch(name);
    let parent = repo.parent().unwrap();
    fs::create_dir_all(parent)?;
    git(
        parent,
        &["clone", "--no-hardlinks", "--", fetch.source, name],
    )?;
    let mut skipped = Vec::new();
    let commit = match fetch.reference {
        Some(reference) => resolve_ref(&repo, reference)?,
        None => {
            let mut found = releases::discover_all(&repo, None)?;
            found.ensure_latest_is_valid()?;
            skipped = std::mem::take(&mut found.skipped);
            found
                .tags
                .pop()
                .require(Error::NoReleases, "no releases")?
                .commit
        }
    };
    git(&repo, &["checkout", "--detach", &commit])?;
    let m = manifest(&repo)?;
    if let Some(expected) = fetch.expected {
        ensure!(
            serde_json::to_value(&m)? == serde_json::to_value(expected)?,
            Catalog,
            "catalog manifest does not match git revision"
        );
    }
    if let Some(expected_files) = fetch.files {
        ensure!(
            catalog::hash_files(&repo.join("dist"))? == *expected_files,
            Catalog,
            "catalog file hashes do not match git dist"
        );
    }
    Ok((repo, m, skipped))
}

/// The new plugin's ID must not collide with anything installed or retained.
fn check_new_id(root: &Path, state: &State, m: &Manifest) -> Result<()> {
    if state.plugins.contains_key(&m.id) {
        return Err(Error::AlreadyInstalled {
            message: format!("already installed: {}", m.id),
            id: m.id.clone(),
        });
    }
    // A legacy ID can be a prefix of a scoped ID; never nest a repository in another.
    ensure!(
        !state
            .plugins
            .keys()
            .any(|id| id.starts_with(&format!("{}/", m.id)) || m.id.starts_with(&format!("{id}/"))),
        Workspace,
        "plugin ID conflicts with an installed namespace path"
    );
    if let Some(other) = state.plugins.keys().find(|id| ids_collide(id, &m.id)) {
        return Err(Error::Workspace(format!(
            "plugin ID {} differs only in letter case from installed {other}",
            m.id
        )));
    }
    ensure_no_case_collision(&root.join("plugins"), &m.id)?;
    let dest = root.join("plugins").join(&m.id);
    if m.id.contains('/') {
        ensure!(
            !dest.parent().unwrap().join(".git").exists(),
            Workspace,
            "namespace path is an existing plugin repository"
        );
    }
    if dest.exists() {
        return Err(Error::AlreadyInstalled {
            message: format!("repository already exists: {}; use restore", m.id),
            id: m.id.clone(),
        });
    }
    Ok(())
}

/// Stage the verified clone as an installed plugin and register it.
fn adopt(
    txn: &mut Txn,
    state: &mut State,
    repo: &Path,
    m: &Manifest,
    source: String,
    catalog: Option<String>,
) -> Result<InstallResult> {
    let current = revision(repo, m)?;
    txn.add_repo(&m.id, repo)?;
    txn.deploy(&m.id)?;
    let mut installed = Installed {
        source: source.clone(),
        current: current.clone(),
        previous: None,
        enabled: true,
        catalog,
        ..Default::default()
    };
    installed.adopt(m);
    state.plugins.insert(m.id.clone(), installed);
    Ok(InstallResult {
        id: m.id.clone(),
        source,
        current,
        fork_of: m.fork_of.clone(),
        skipped: Vec::new(),
    })
}

/// One catalog per workspace: plugins from another catalog must not be mixed in.
fn ensure_single_catalog(state: &State, origin: &str) -> Result<()> {
    if let Some((id, other)) = state.plugins.iter().find_map(|(id, p)| {
        p.catalog
            .as_deref()
            .filter(|c| *c != origin)
            .map(|c| (id, c))
    }) {
        return Err(Error::Catalog(format!(
            "this workspace uses catalog {other} ({id} was installed from it); \
             mixing catalogs is not supported, select that catalog or uninstall its plugins"
        )));
    }
    Ok(())
}

fn refuse_dependents(state: &State, id: &str, action: &str) -> Result<()> {
    let dependents = deps::dependents(state, id);
    if dependents.is_empty() {
        return Ok(());
    }
    Err(Error::HasDependents {
        message: format!(
            "cannot {action} {id}: required by {}; use --force to {action} anyway",
            dependents.join(", ")
        ),
        id: id.into(),
        dependents,
    })
}

/// A consumer whose requirement the new provider's version does not satisfy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IncompatibleConsumer {
    pub id: String,
    pub requirement: String,
}
/// What a replacement (or switching back) changes.
#[derive(Clone, Debug, Serialize)]
pub struct ReplacementPlan {
    pub original: String,
    /// Plugin that provides `original` afterwards (the original itself when switching back).
    pub provider: String,
    /// Version consumers see: the fork's declared `replaces.version`, or the original's.
    pub version: String,
    /// Current provider if `original` is replaced now.
    pub current: Option<String>,
    /// Installed plugins that depend on `original`.
    pub consumers: Vec<String>,
    /// Consumers whose requirement `version` does not satisfy (need confirmation).
    pub incompatible: Vec<IncompatibleConsumer>,
}
impl ReplacementPlan {
    fn refuse_unless(&self, confirm: bool) -> Result<()> {
        if confirm || self.incompatible.is_empty() {
            return Ok(());
        }
        let list = self
            .incompatible
            .iter()
            .map(|c| format!("{} needs {} {}", c.id, self.original, c.requirement))
            .collect::<Vec<_>>()
            .join("; ");
        Err(Error::IncompatibleReplacement {
            message: format!(
                "{} provides {} {}, which some consumers do not accept: {list}; confirm to apply anyway",
                self.provider, self.original, self.version
            ),
            id: self.original.clone(),
            consumers: self.incompatible.iter().map(|c| c.id.clone()).collect(),
        })
    }
}

fn replacement_plan(
    state: &State,
    original: &str,
    replacement: Option<&str>,
) -> Result<ReplacementPlan> {
    valid_id(original)?;
    let current = state.replacements.get(original).cloned();
    let (provider, version) = match replacement {
        Some(fork) => {
            valid_id(fork)?;
            ensure!(
                fork != original,
                InvalidArgument,
                "cannot replace {original} by itself"
            );
            let installed = state
                .plugins
                .get(fork)
                .ok_or_else(|| Error::NotInstalled { id: fork.into() })?;
            let declared = installed
                .replaces
                .as_ref()
                .filter(|r| r.id == original)
                .require(
                    Error::InvalidArgument,
                    format!(
                        "{fork} does not declare that it replaces {original} \
                         (plugin.json \"replaces\": {{\"id\": \"{original}\", \"version\": \"...\"}})"
                    ),
                )?;
            if let Some(other) = state.replacements.get(fork) {
                fail!(
                    InvalidArgument,
                    "cyclic replacement: {fork} is itself replaced by {other}"
                );
            }
            if let Some((replaced, _)) = state.replacements.iter().find(|(_, p)| *p == original) {
                fail!(
                    InvalidArgument,
                    "cyclic replacement: {original} provides {replaced}"
                );
            }
            ensure!(
                !installed.dependencies.contains_key(original),
                InvalidArgument,
                "cyclic replacement: {fork} depends on {original}"
            );
            (fork.to_owned(), declared.version.clone())
        }
        None => {
            current.as_deref().require(
                Error::InvalidArgument,
                format!("{original} is not replaced"),
            )?;
            let installed = state
                .plugins
                .get(original)
                .ok_or_else(|| Error::NotInstalled {
                    id: original.into(),
                })?;
            (original.to_owned(), installed.current.version.clone())
        }
    };
    let consumers: Vec<String> = state
        .plugins
        .iter()
        .filter(|(id, p)| **id != provider && p.dependencies.contains_key(original))
        .map(|(id, _)| id.clone())
        .collect();
    let incompatible = consumers
        .iter()
        .filter_map(|id| {
            let requirement = &state.plugins[id].dependencies[original];
            (!deps::matches(requirement, &version)).then(|| IncompatibleConsumer {
                id: id.clone(),
                requirement: requirement.clone(),
            })
        })
        .collect();
    // The new provider's own dependencies must work in the resulting workspace.
    let mut next = state.clone();
    match replacement {
        Some(fork) => {
            next.replacements.insert(original.into(), fork.into());
        }
        None => {
            next.replacements.remove(original);
        }
    }
    let broken: Vec<String> = deps::status(&next, &next.plugins[&provider])
        .into_iter()
        .filter(|s| !s.satisfied)
        .map(|s| {
            format!(
                "{} {} ({})",
                s.id,
                s.requirement,
                s.problem.unwrap_or_default()
            )
        })
        .collect();
    ensure!(
        broken.is_empty(),
        DependencyConflict,
        "{provider} has unsatisfied dependencies: {}",
        broken.join(", ")
    );
    Ok(ReplacementPlan {
        original: original.into(),
        provider,
        version,
        current,
        consumers,
        incompatible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        Author, TempDir, git as fixture_git, manifest as make_manifest, tree_hash, write_manifest,
    };
    use std::fmt::Debug;

    fn assert_send_sync<T: Send + Sync>() {}
    #[test]
    fn manager_is_send_and_sync() {
        assert_send_sync::<Manager>();
    }

    /// Author repository + workspace path in one temp directory.
    struct Ws {
        dir: TempDir,
        root: PathBuf,
        author: Author,
    }
    impl Ws {
        fn new(id: &str) -> Self {
            let dir = TempDir::new("workspace");
            let author = Author::new(dir.join("author"), id);
            let root = dir.join("workspace");
            Self { dir, root, author }
        }
        fn open(&self) -> Manager {
            reopen(&self.root).unwrap()
        }
        /// Workspace with the author's latest release installed.
        fn installed(id: &str) -> (Self, Manager) {
            let ws = Self::new(id);
            let m = ws.open();
            m.install(ws.author.source()).unwrap();
            (ws, m)
        }
        fn repo(&self) -> PathBuf {
            self.root.join("plugins").join(&self.author.id)
        }
        fn dist(&self) -> PathBuf {
            self.root.join("dist").join(&self.author.id)
        }
        fn other_author(&self, name: &str, id: &str) -> Author {
            Author::new(self.dir.join(name), id)
        }
        fn no_leftovers(&self) {
            assert!(
                !self.root.join(txn::DIR).exists(),
                "transaction directory left behind"
            );
        }
        /// Run a failing operation and prove the workspace is byte-identical afterwards.
        fn refuses<T: Debug>(&self, kind: &str, op: impl FnOnce() -> Result<T>) -> Error {
            let before = tree_hash(&self.root);
            let error = op().expect_err("operation must fail");
            assert_eq!(error.kind(), kind, "{error}");
            assert_eq!(
                tree_hash(&self.root),
                before,
                "workspace changed by failed op: {error}"
            );
            self.no_leftovers();
            error
        }
    }
    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }
    /// Reopen after a handle was dropped. Other test threads spawn subprocesses; while a
    /// spawn is in flight the child briefly shares the just-closed lock descriptor
    /// (flock semantics), so an immediate reopen can report `locked`. Retry only that.
    fn reopen(root: &Path) -> Result<Manager> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match Manager::open(root) {
                Err(Error::Locked(_)) if std::time::Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                other => return other,
            }
        }
    }

    // ---- helpers --------------------------------------------------------------------

    #[test]
    fn copy_tree_copies_nested_files_and_refuses_symlinks() {
        let dir = TempDir::new("workspace");
        let from = dir.join("from");
        fs::create_dir_all(from.join("a/b")).unwrap();
        fs::create_dir_all(from.join("empty")).unwrap();
        fs::write(from.join("a/b/c.txt"), "c").unwrap();
        fs::write(from.join("top"), [0u8, 255]).unwrap();
        copy_tree(&from, &dir.join("to"), false).unwrap();
        assert_eq!(tree_hash(&dir.join("to")), tree_hash(&from));

        std::os::unix::fs::symlink("top", from.join("a/link")).unwrap();
        let error = copy_tree(&from, &dir.join("to2"), false).unwrap_err();
        assert_eq!(error.kind(), "unsupported");
        assert!(error.to_string().contains("symlinks are not supported"));
        // Repository copies keep symlinks as symlinks.
        copy_tree(&from, &dir.join("to4"), true).unwrap();
        assert_eq!(
            fs::read_link(dir.join("to4/a/link")).unwrap(),
            Path::new("top")
        );
        let error = copy_tree(&dir.join("missing"), &dir.join("to3"), false).unwrap_err();
        assert_eq!(error.kind(), "io");
        assert!(error.to_string().starts_with("missing "));
    }

    #[test]
    fn exact_path_matches_spelling_component_by_component() {
        let dir = TempDir::new("workspace");
        fs::create_dir_all(dir.join("plugins/vendor/module/.git")).unwrap();
        let plugins = dir.join("plugins");
        assert!(exact_path(&plugins, "vendor/module").unwrap());
        assert!(exact_path(&plugins, "vendor").unwrap());
        for other in ["Vendor/module", "vendor/Module", "vendor/mod", "x"] {
            assert!(!exact_path(&plugins, other).unwrap(), "{other}");
        }
        assert!(!exact_path(&dir.join("missing"), "vendor").unwrap());
        assert!(plugin_repo(dir.path(), "vendor/module").is_ok());
        for other in ["vendor", "Vendor/module", "vendor/other"] {
            let error = plugin_repo(dir.path(), other).unwrap_err();
            assert!(
                matches!(&error, Error::RepositoryNotFound { id } if id == other),
                "{error:?}"
            );
        }
    }

    // ---- open -----------------------------------------------------------------------

    #[test]
    fn open_creates_the_workspace_layout_and_lock() {
        let dir = TempDir::new("workspace");
        let root = dir.join("nested/workspace");
        let options = Options {
            catalog: Some("catalog-local".into()),
        };
        let m = Manager::open_with(&root, options).unwrap();
        assert_eq!(m.root(), root);
        assert_eq!(m.options().catalog.as_deref(), Some("catalog-local"));
        assert_eq!(read(&root.join("state.json")), "{\n  \"plugins\": {}\n}");
        assert!(root.join("plugins").is_dir() && root.join("dist").is_dir());
        assert!(dir.join("nested/.workspace.lock").exists());
        assert!(m.list().unwrap().is_empty());
        let error = Manager::open(&root).err().unwrap();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("locked", "workspace is already in use".into())
        );
        drop(m);
        // The lock is released with the handle; reopening keeps the state.
        let before = tree_hash(&root);
        drop(reopen(&root).unwrap());
        assert_eq!(tree_hash(&root), before);
        assert_eq!(Manager::open("/").err().unwrap().kind(), "workspace");
    }

    #[test]
    fn open_refuses_foreign_directories_and_bad_state() {
        let dir = TempDir::new("workspace");
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        assert!(
            reopen(&root).is_ok(),
            "an empty directory becomes a workspace"
        );

        let foreign = dir.join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("notes.txt"), "user data").unwrap();
        let before = tree_hash(&foreign);
        let error = Manager::open(&foreign).err().unwrap();
        assert_eq!(error.kind(), "workspace");
        assert_eq!(tree_hash(&foreign), before);

        for (text, kind) in [
            ("{truncated", "invalid_data"),
            (
                r#"{"plugins":{"../evil":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true}}}"#,
                "invalid_id",
            ),
            (
                r#"{"plugins":{"a/b/c":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true}}}"#,
                "invalid_id",
            ),
        ] {
            fs::write(root.join("state.json"), text).unwrap();
            let before = tree_hash(&root);
            assert_eq!(reopen(&root).err().unwrap().kind(), kind, "{text}");
            assert_eq!(tree_hash(&root), before);
        }
    }

    /// Run `op` with a simulated crash after `renames` commit renames.
    fn crash_after<T>(renames: usize, op: impl FnOnce() -> Result<T>) -> Error {
        txn::CRASH_AFTER.with(|c| c.set(Some(renames)));
        let result = op();
        txn::CRASH_AFTER.with(|c| c.set(None));
        result.err().expect("simulated crash")
    }

    #[test]
    fn open_rolls_an_interrupted_commit_forward() {
        let id = "dan/demo1";
        // An update commit makes 5 renames (repo out/in, dist out/in, state.json).
        for renames in 0..5 {
            let (ws, m) = Ws::installed(id);
            ws.author.release("0.2.0");
            crash_after(renames, || m.update(id));
            assert!(ws.root.join(txn::DIR).join("journal.json").exists());
            drop(m);
            let m = ws.open();
            let p = &m.list().unwrap()[0];
            assert_eq!(p.current.version, "0.2.0", "crash after {renames}");
            assert_eq!(p.previous.as_ref().unwrap().version, "0.1.0");
            assert_eq!(
                fixture_git(&ws.repo(), &["rev-parse", "HEAD"]),
                ws.author.head()
            );
            assert_eq!(
                tree_hash(&ws.dist()),
                tree_hash(&ws.author.path.join("dist"))
            );
            ws.no_leftovers();
        }
    }

    #[test]
    fn open_discards_an_uncommitted_transaction() {
        let (ws, m) = Ws::installed("dan/demo1");
        drop(m);
        let before = tree_hash(&ws.root);
        // Crash while staging: no journal yet.
        fs::create_dir_all(ws.root.join(txn::DIR).join("new/plugins/dan/half")).unwrap();
        let m = ws.open();
        assert_eq!(tree_hash(&ws.root), before);
        ws.no_leftovers();
        // A failed operation in the same handle recovers too.
        drop(m);
        fs::create_dir_all(ws.root.join(txn::DIR).join("scratch")).unwrap();
        let m = ws.open();
        m.disable("dan/demo1").unwrap();
        ws.no_leftovers();
    }

    #[test]
    fn neighbours_named_like_old_stage_folders_are_never_touched() {
        let (ws, m) = Ws::installed("dan/demo1");
        drop(m);
        for name in [".workspace.stage", ".workspace.backup"] {
            fs::create_dir_all(ws.dir.join(name)).unwrap();
            fs::write(ws.dir.join(name).join("user.txt"), "user data").unwrap();
        }
        let m = ws.open();
        m.disable("dan/demo1").unwrap();
        for name in [".workspace.stage", ".workspace.backup"] {
            assert_eq!(read(&ws.dir.join(name).join("user.txt")), "user data");
        }
    }

    // ---- failures change nothing ----------------------------------------------------

    #[test]
    fn every_operation_rejects_invalid_ids_without_changes() {
        let (ws, m) = Ws::installed("dan/demo1");
        let dest = ws.dir.join("fork");
        for id in ["", "../evil", "a/b/c", "dan/demo 1", "dan\\demo1"] {
            ws.refuses("invalid_id", || m.update(id));
            ws.refuses("invalid_id", || m.update_channel(id, None, true));
            ws.refuses("invalid_id", || m.rollback(id));
            ws.refuses("invalid_id", || m.enable(id));
            ws.refuses("invalid_id", || m.disable(id));
            ws.refuses("invalid_id", || m.forget(id));
            ws.refuses("invalid_id", || m.restore(id));
            ws.refuses("invalid_id", || m.uninstall_plan(id));
            ws.refuses("invalid_id", || m.uninstall(id, true));
            ws.refuses("invalid_id", || m.fork_in_namespace(id, &dest, "me"));
        }
        assert!(!dest.exists());
    }

    #[test]
    fn every_operation_on_a_missing_id_fails_without_changes() {
        let (ws, m) = Ws::installed("dan/demo1");
        let id = "dan/missing";
        for error in [
            ws.refuses("not_installed", || m.update(id)),
            ws.refuses("not_installed", || m.update_ref(id, Some("HEAD"))),
            ws.refuses("not_installed", || m.update_channel(id, None, true)),
            ws.refuses("not_installed", || m.rollback(id)),
            ws.refuses("not_installed", || m.enable(id)),
            ws.refuses("not_installed", || m.disable(id)),
            ws.refuses("not_installed", || m.forget(id)),
            ws.refuses("not_installed", || {
                m.fork_in_namespace(id, &ws.dir.join("f"), "me")
            }),
        ] {
            assert!(
                matches!(&error, Error::NotInstalled { id: e } if e == id),
                "{error:?}"
            );
            assert_eq!(error.to_string(), format!("plugin {id} is not installed"));
        }
        for error in [
            ws.refuses("repository_not_found", || m.uninstall_plan(id)),
            ws.refuses("repository_not_found", || m.uninstall(id, false)),
            ws.refuses("repository_not_found", || m.uninstall(id, true)),
            // A namespace directory is not a plugin.
            ws.refuses("repository_not_found", || m.uninstall("dan", true)),
        ] {
            assert!(matches!(error, Error::RepositoryNotFound { .. }));
        }
        // Restore of an unknown ID fails too (see the ignored test for its kind).
        let before = tree_hash(&ws.root);
        assert!(m.restore(id).is_err());
        assert_eq!(tree_hash(&ws.root), before);
        ws.no_leftovers();
    }

    #[test]
    fn restore_of_a_missing_repository_is_repository_not_found() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.refuses("repository_not_found", || m.restore("dan/missing"));
    }

    #[test]
    fn corrupt_state_blocks_every_operation_and_is_not_overwritten() {
        let (ws, m) = Ws::installed("dan/demo1");
        let id = "dan/demo1";
        fs::write(ws.root.join("state.json"), "{\"plugins\": {").unwrap();
        let other = ws.other_author("other", "dan/other");
        ws.refuses("invalid_data", || m.state());
        ws.refuses("invalid_data", || m.list());
        ws.refuses("invalid_data", || m.check());
        ws.refuses("invalid_data", || m.install(other.source()));
        ws.refuses("invalid_data", || m.update(id));
        ws.refuses("invalid_data", || m.rollback(id));
        ws.refuses("invalid_data", || m.enable(id));
        ws.refuses("invalid_data", || m.forget(id));
        ws.refuses("invalid_data", || m.restore(id));
        ws.refuses("invalid_data", || m.uninstall_plan(id));
        ws.refuses("invalid_data", || m.uninstall(id, true));
        ws.refuses("invalid_data", || {
            m.fork_in_namespace(id, &ws.dir.join("f"), "me")
        });
        assert_eq!(read(&ws.root.join("state.json")), "{\"plugins\": {");
        assert!(!ws.dir.join("f").exists());
    }

    // ---- install --------------------------------------------------------------------

    #[test]
    fn install_clones_checks_out_the_latest_release_and_deploys_dist() {
        let ws = Ws::new("dan/demo1");
        ws.author.release("1.9.0");
        let c110 = ws.author.release("1.10.0");
        ws.author.commit("2.0.0"); // untagged HEAD is not a release
        let m = ws.open();
        // A non-canonical spelling of a local source is stored canonically.
        let spelled = ws.dir.join("author/../author");
        let result = m.install(spelled.to_str().unwrap()).unwrap();
        assert_eq!(result.id, "dan/demo1");
        assert_eq!(result.source, ws.author.source());
        assert_eq!(
            result.current,
            Revision {
                version: "1.10.0".into(),
                commit: c110.clone()
            }
        );
        assert!(result.fork_of.is_none());

        let state = m.state().unwrap();
        let p = &state.plugins["dan/demo1"];
        assert!(p.enabled && p.previous.is_none() && p.fork_of.is_none());
        assert_eq!(p.current.commit, c110);
        assert_eq!(fixture_git(&ws.repo(), &["rev-parse", "HEAD"]), c110);
        assert_eq!(
            read(&ws.dist().join("index.js")),
            "export const v = \"1.10.0\";\n"
        );
        assert_eq!(
            fixture_git(&ws.repo(), &["remote", "get-url", "origin"]),
            ws.author.source()
        );
        let list = m.list().unwrap();
        assert_eq!(
            (list.len(), list[0].current.version.as_str()),
            (1, "1.10.0")
        );
        ws.no_leftovers();
    }

    #[test]
    fn install_channels_and_explicit_refs() {
        let ws = Ws::new("demo1");
        let first = ws.author.head();
        ws.author.release("0.2.0");
        let head = ws.author.commit("0.3.0");
        let m = ws.open();
        ws.refuses("invalid_argument", || {
            m.install_channel(ws.author.source(), Some("v0.1.0"), true)
        });
        let r = m
            .install_channel(ws.author.source(), Some("v0.1.0"), false)
            .unwrap();
        assert_eq!(
            (r.id.as_str(), r.current.commit.as_str()),
            ("demo1", first.as_str())
        );
        assert!(ws.root.join("plugins/demo1/.git").is_dir());
        assert!(ws.root.join("dist/demo1/index.js").is_file());
        m.uninstall("demo1", false).unwrap();
        let r = m.install_channel(ws.author.source(), None, true).unwrap();
        assert_eq!(
            (r.current.version.as_str(), r.current.commit),
            ("0.3.0", head)
        );
    }

    #[test]
    fn install_refusals_leave_the_workspace_unchanged() {
        let (ws, m) = Ws::installed("dan/demo1");
        let error = ws.refuses("already_installed", || m.install(ws.author.source()));
        assert_eq!(error.to_string(), "already installed: dan/demo1");
        ws.refuses("invalid_argument", || m.install("-uevil"));
        ws.refuses("invalid_argument", || {
            m.install_ref(ws.author.source(), Some("-x"), None)
        });
        ws.refuses("git", || {
            m.install(ws.dir.join("missing").to_str().unwrap())
        });
        ws.refuses("git", || {
            m.install_ref(ws.author.source(), Some("no-such-ref"), None)
        });

        let untagged = Author::untagged(ws.dir.join("untagged"), "dan/untagged", "0.1.0");
        ws.refuses("no_releases", || m.install(untagged.source()));

        let deps = Author::new(ws.dir.join("deps"), "dan/deps");
        let mut with_deps = make_manifest("dan/deps", "0.2.0");
        with_deps
            .dependencies
            .insert("dan/missing".into(), "^1.0".into());
        write_manifest(&deps.path, &with_deps);
        fixture_git(&deps.path, &["commit", "-q", "-am", "deps"]);
        let error = ws.refuses("dependency_conflict", || {
            m.install_ref(deps.source(), Some("HEAD"), None)
        });
        assert!(error.to_string().contains("dan/missing"), "{error}");

        let no_dist = Author::new(ws.dir.join("nodist"), "dan/nodist");
        fixture_git(&no_dist.path, &["rm", "-q", "-r", "dist"]);
        write_manifest(&no_dist.path, &make_manifest("dan/nodist", "0.2.0"));
        fixture_git(&no_dist.path, &["commit", "-q", "-am", "no dist"]);
        ws.refuses("invalid_manifest", || {
            m.install_ref(no_dist.source(), Some("HEAD"), None)
        });

        // A legacy ID is a prefix of an installed scoped ID (and vice versa).
        let legacy = ws.other_author("legacy", "dan");
        let error = ws.refuses("workspace", || m.install(legacy.source()));
        assert!(error.to_string().contains("namespace path"));
        let legacy2 = ws.other_author("legacy2", "solo");
        m.install(legacy2.source()).unwrap();
        let nested = ws.other_author("nested", "solo/inner");
        ws.refuses("workspace", || m.install(nested.source()));

        // The expected manifest must match the Git revision exactly.
        let other = ws.other_author("other", "dan/other");
        let wrong = make_manifest("dan/other", "9.9.9");
        let error = ws.refuses("catalog", || {
            m.install_ref(other.source(), None, Some(&wrong))
        });
        assert_eq!(
            error.to_string(),
            "catalog manifest does not match git revision"
        );
        let mut files = BTreeMap::new();
        files.insert(
            "index.js".to_owned(),
            catalog::FileDigest {
                sha256: "0".repeat(64),
                size: 1,
            },
        );
        let release = catalog::Release {
            manifest: make_manifest("dan/other", "0.1.0"),
            source: other.source().into(),
            commit: other.head(),
            reference: "dan/other@0.1.0".into(),
            files,
            build: None,
        };
        let error = ws.refuses("catalog", || m.install_release(&release));
        assert_eq!(
            error.to_string(),
            "catalog file hashes do not match git dist"
        );
    }

    #[test]
    fn forgotten_repository_blocks_reinstall_until_restored() {
        let (ws, m) = Ws::installed("dan/demo1");
        m.forget("dan/demo1").unwrap();
        assert!(m.list().unwrap().is_empty());
        assert!(ws.repo().join(".git").is_dir() && ws.dist().join("index.js").is_file());
        let error = ws.refuses("already_installed", || m.install(ws.author.source()));
        assert!(error.to_string().contains("use restore"), "{error}");
        ws.refuses("not_installed", || m.forget("dan/demo1"));
        let restored = m.restore("dan/demo1").unwrap();
        assert_eq!(restored.source, ws.author.source());
        assert_eq!(restored.current.version, "0.1.0");
        let p = &m.state().unwrap().plugins["dan/demo1"];
        assert!(p.enabled && p.previous.is_none());
        let error = ws.refuses("already_installed", || m.restore("dan/demo1"));
        assert_eq!(error.to_string(), "plugin dan/demo1 is already registered");
    }

    #[test]
    fn restore_of_registered_plugin_without_clone_reports_the_missing_repository() {
        let (ws, m) = Ws::installed("dan/demo1");
        fs::remove_dir_all(ws.repo()).unwrap();
        let error = ws.refuses("repository_not_found", || m.restore("dan/demo1"));
        assert!(error.to_string().contains("dan/demo1"), "{error}");
    }

    #[test]
    fn restore_refuses_dirty_or_mismatched_repositories() {
        let (ws, m) = Ws::installed("dan/demo1");
        m.forget("dan/demo1").unwrap();
        fs::write(ws.repo().join("dist/index.js"), "local edit").unwrap();
        let error = ws.refuses("local_changes", || m.restore("dan/demo1"));
        assert!(matches!(error, Error::LocalChanges { items, .. } if items.len() == 1));
        fixture_git(&ws.repo(), &["checkout", "-q", "--", "dist/index.js"]);
        // The repository's manifest names another plugin.
        copy_tree(&ws.repo(), &ws.root.join("plugins/dan/alias2"), true).unwrap();
        let error = ws.refuses("invalid_manifest", || m.restore("dan/alias2"));
        assert_eq!(error.to_string(), "plugin id mismatch");
    }

    // ---- update / rollback ----------------------------------------------------------

    #[test]
    fn update_and_rollback_swap_revisions_and_redeploy() {
        let (ws, m) = Ws::installed("dan/demo1");
        let id = "dan/demo1";
        let v1 = ws.author.head();
        let noop = m.update(id).unwrap();
        assert!(!noop.changed && noop.from == noop.to);
        let v2 = ws.author.release("0.2.0");
        m.disable(id).unwrap();
        let change = m.update(id).unwrap();
        assert!(change.changed);
        assert_eq!(
            (change.from.commit.as_str(), change.to.commit.as_str()),
            (v1.as_str(), v2.as_str())
        );
        let p = m.state().unwrap().plugins[id].clone();
        assert_eq!(p.current.version, "0.2.0");
        assert_eq!(p.previous.as_ref().unwrap().commit, v1);
        assert!(!p.enabled, "update keeps the enabled flag");
        assert_eq!(
            read(&ws.dist().join("index.js")),
            "export const v = \"0.2.0\";\n"
        );

        // Rollback works offline and swaps current/previous.
        fs::rename(&ws.author.path, ws.dir.join("offline")).unwrap();
        let back = m.rollback(id).unwrap();
        assert_eq!(
            (back.from.version.as_str(), back.to.version.as_str()),
            ("0.2.0", "0.1.0")
        );
        assert_eq!(
            read(&ws.dist().join("index.js")),
            "export const v = \"0.1.0\";\n"
        );
        let p = m.state().unwrap().plugins[id].clone();
        assert_eq!(p.previous.unwrap().version, "0.2.0");
        let forward = m.rollback(id).unwrap();
        assert_eq!(forward.to.commit, v2);
        // Fetching from the missing origin fails without changes.
        ws.refuses("git", || m.update(id));
    }

    #[test]
    fn update_refusals_leave_the_workspace_unchanged() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        ws.refuses("no_previous_version", || m.rollback(id));
        ws.refuses("invalid_argument", || {
            m.update_channel(id, Some("main"), true)
        });
        ws.refuses("invalid_argument", || m.update_ref(id, Some("-x")));

        // Upstream: same version on a new commit, then a commit with another ID.
        let same = same_version_commit(&ws.author);
        let error = ws.refuses("version_conflict", || m.update_ref(id, Some(&same)));
        assert_eq!(error.to_string(), "upstream changed without a version bump");
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.2.0"));
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "rename"]);
        let renamed = ws.author.head();
        let error = ws.refuses("invalid_manifest", || m.update_ref(id, Some(&renamed)));
        assert_eq!(error.to_string(), "plugin id changed upstream");
    }

    /// New upstream commit that keeps the manifest version.
    fn same_version_commit(author: &Author) -> String {
        fs::write(author.path.join("dist/extra.js"), "extra").unwrap();
        fixture_git(&author.path, &["add", "--all"]);
        fixture_git(&author.path, &["commit", "-q", "-m", "same version"]);
        author.head()
    }

    #[test]
    fn stable_update_refuses_a_release_whose_commit_changed() {
        let ws = Ws::new("dan/demo1");
        same_version_commit(&ws.author);
        let m = ws.open();
        // Installed 0.1.0 from HEAD, but the 0.1.0 release tag is another commit.
        m.install_channel(ws.author.source(), None, true).unwrap();
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert_eq!(
            error.to_string(),
            "release commit changed for installed version"
        );
    }

    #[test]
    fn update_refuses_moved_fetched_but_not_installed_release() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.author.release("0.1.1");
        let original = ws.author.release("0.1.2");
        m.update_ref("dan/demo1", Some("dan/demo1@0.1.1")).unwrap();
        assert_eq!(
            resolve_ref(&ws.repo(), "dan/demo1@0.1.2").unwrap(),
            original
        );
        same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "dan/demo1@0.1.2"]);
        for reference in [None, Some("dan/demo1@0.1.2")] {
            ws.refuses("version_conflict", || m.update_ref("dan/demo1", reference));
            assert_eq!(
                resolve_ref(&ws.repo(), "dan/demo1@0.1.2").unwrap(),
                original
            );
        }
    }

    #[test]
    fn stable_update_refuses_scoped_tag_with_another_manifest_id() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.author.commit("0.1.1");
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.1.1"));
        fixture_git(&ws.author.path, &["add", "."]);
        fixture_git(&ws.author.path, &["commit", "-qm", "rename"]);
        ws.author.tag("0.1.1");
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert!(error.to_string().contains("manifest ID dan/renamed"));
    }

    #[test]
    fn stable_update_follows_deleted_and_moved_upstream_tags() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.author.release("0.2.0");
        m.update("dan/demo1").unwrap();
        // Moved release tag: same version, other commit.
        same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "dan/demo1@0.2.0"]);
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert_eq!(
            error.to_string(),
            "release commit changed for installed version"
        );
        // Deleted release tag: the installed version is no longer the latest release.
        fixture_git(&ws.author.path, &["tag", "-d", "dan/demo1@0.2.0"]);
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert!(error.to_string().starts_with("latest release is older"));
    }

    #[test]
    fn direct_install_reports_skipped_broken_release_tags() {
        let ws = Ws::new("dan/demo1");
        // An old tag whose manifest disagrees with the tag version.
        fixture_git(&ws.author.path, &["tag", "dan/demo1@0.0.9"]);
        ws.author.release("0.2.0");
        let result = ws.open().install(ws.author.source()).unwrap();
        assert_eq!(result.current.version, "0.2.0");
        let names: Vec<_> = result.skipped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["dan/demo1@0.0.9"]);
    }

    #[test]
    fn stable_update_never_downgrades() {
        let ws = Ws::new("dan/demo1");
        ws.author.commit("0.5.0");
        let m = ws.open();
        m.install_channel(ws.author.source(), None, true).unwrap();
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert!(error.to_string().starts_with("latest release is older"));
        // An explicit ref may downgrade.
        let change = m.update_ref("dan/demo1", Some("dan/demo1@0.1.0")).unwrap();
        assert_eq!(change.to.version, "0.1.0");
    }

    #[test]
    fn local_work_blocks_update_and_rollback() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        ws.author.release("0.2.0");
        m.update(id).unwrap();
        ws.author.release("0.3.0");
        fs::write(ws.repo().join("untracked.txt"), "keep").unwrap();
        for error in [
            ws.refuses("local_changes", || m.update(id)),
            ws.refuses("local_changes", || m.rollback(id)),
        ] {
            let Error::LocalChanges { id: e, items, .. } = error else {
                panic!()
            };
            assert_eq!(e, id);
            assert_eq!(
                items,
                [LostWork::Untracked {
                    path: "untracked.txt".into()
                }]
            );
        }
        fs::remove_file(ws.repo().join("untracked.txt")).unwrap();
        // A local commit moves HEAD away from the installed revision.
        fixture_git(
            &ws.repo(),
            &["commit", "-q", "--allow-empty", "-m", "local"],
        );
        for error in [
            ws.refuses("local_changes", || m.update(id)),
            ws.refuses("local_changes", || m.rollback(id)),
        ] {
            let Error::LocalChanges { items, .. } = error else {
                panic!()
            };
            assert!(
                matches!(&items[..], [LostWork::HeadMoved { .. }]),
                "{items:?}"
            );
        }
    }

    #[test]
    fn dev_channel_follows_origin_head_and_refuses_diverged_history() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_channel(ws.author.source(), None, true).unwrap();
        let next = ws.author.commit("0.2.0");
        let change = m.update_channel(id, None, true).unwrap();
        assert_eq!(
            (change.to.commit.as_str(), change.to.version.as_str()),
            (next.as_str(), "0.2.0")
        );
        // On dev a new commit is an update even without a version bump.
        let same = same_version_commit(&ws.author);
        let change = m.update_channel(id, None, true).unwrap();
        assert!(change.changed);
        assert_eq!(
            (change.to.commit.as_str(), change.to.version.as_str()),
            (same.as_str(), "0.2.0")
        );
        m.rollback(id).unwrap();
        m.update_channel(id, None, true).unwrap();
        // Rewrite upstream history: the installed commit is no longer an ancestor.
        fixture_git(&ws.author.path, &["reset", "-q", "--hard", "HEAD~1"]);
        ws.author.commit("0.3.0");
        let error = ws.refuses("version_conflict", || m.update_channel(id, None, true));
        assert!(
            error.to_string().starts_with("upstream history diverged"),
            "{error}"
        );
    }

    #[test]
    fn update_to_a_branch_uses_the_fetched_upstream_branch() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        let next = ws.author.commit("0.2.0");
        let change = m.update_ref(id, Some("main")).unwrap();
        assert!(change.changed, "branch moved upstream");
        assert_eq!(change.to.commit, next);
        // HEAD means upstream HEAD, not the detached checkout.
        let head = ws.author.commit("0.3.0");
        assert_eq!(m.update_ref(id, Some("HEAD")).unwrap().to.commit, head);
        ws.refuses("invalid_argument", || {
            m.update_ref(id, Some("refs/heads/main"))
        });
        ws.refuses("invalid_argument", || m.update_ref(id, Some("no-such-ref")));
    }

    // ---- enable / forget / uninstall ------------------------------------------------

    #[test]
    fn enable_and_disable_change_only_the_flag() {
        let (ws, m) = Ws::installed("dan/demo1");
        let before = serde_json::to_value(&m.state().unwrap().plugins["dan/demo1"]).unwrap();
        let payload = tree_hash(&ws.root.join("plugins"));
        m.disable("dan/demo1").unwrap();
        m.disable("dan/demo1").unwrap();
        let mut after = serde_json::to_value(&m.state().unwrap().plugins["dan/demo1"]).unwrap();
        assert_eq!(after["enabled"], false);
        after["enabled"] = true.into();
        assert_eq!(after, before);
        m.set_enabled("dan/demo1", true).unwrap();
        assert!(m.list().unwrap()[0].enabled);
        assert_eq!(tree_hash(&ws.root.join("plugins")), payload);
        ws.no_leftovers();
    }

    #[test]
    fn uninstall_plan_is_read_only_and_lists_paths() {
        let (ws, m) = Ws::installed("dan/demo1");
        let before = tree_hash(&ws.root);
        let plan = m.uninstall_plan("dan/demo1").unwrap();
        assert_eq!(tree_hash(&ws.root), before);
        assert!(plan.registered && plan.lost_work.is_empty());
        assert_eq!(
            plan.deletes,
            [
                ws.repo().to_string_lossy().into_owned(),
                ws.dist().to_string_lossy().into_owned()
            ]
        );
        fs::remove_dir_all(ws.dist()).unwrap();
        m.forget("dan/demo1").unwrap();
        let plan = m.uninstall_plan("dan/demo1").unwrap();
        assert!(!plan.registered);
        assert_eq!(plan.deletes, [ws.repo().to_string_lossy().into_owned()]);
        m.uninstall("dan/demo1", false).unwrap();
        assert!(!ws.repo().exists() && !ws.dist().exists());
        assert!(m.list().unwrap().is_empty());
        ws.no_leftovers();
    }

    #[test]
    fn uninstall_keeps_other_plugins() {
        let (ws, m) = Ws::installed("dan/demo1");
        let other = ws.other_author("other", "dan/other");
        m.install(other.source()).unwrap();
        let other_repo = tree_hash(&ws.root.join("plugins/dan/other"));
        m.uninstall("dan/demo1", false).unwrap();
        assert_eq!(tree_hash(&ws.root.join("plugins/dan/other")), other_repo);
        assert!(ws.root.join("dist/dan/other/index.js").is_file());
        assert_eq!(m.list().unwrap().len(), 1);
    }

    // ---- catalog-backed operations --------------------------------------------------

    #[test]
    fn catalog_operations_need_a_configured_catalog() {
        let (ws, m) = Ws::installed("dan/demo1");
        let error = ws.refuses("invalid_argument", || m.check());
        assert_eq!(error.to_string(), "no catalog configured");
        ws.refuses("invalid_argument", || m.browse_catalog());
        ws.refuses("invalid_argument", || m.catalog_release("dan/demo1", None));
        ws.refuses("invalid_argument", || {
            m.install_from_catalog("dan/demo1", None)
        });
        drop(m);
        // An empty workspace never needs the catalog for check.
        let empty = Manager::open(ws.dir.join("empty")).unwrap();
        assert!(empty.check().unwrap().is_empty());
    }

    #[test]
    fn install_from_catalog_verifies_and_check_compares() {
        let ws = Ws::new("dan/demo1");
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "dan/demo1@0.1.0").unwrap();
        ws.author.release("0.2.0");
        catalog::register(&location, ws.author.source(), "dan/demo1@0.2.0").unwrap();
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location.clone()),
            },
        )
        .unwrap();
        assert_eq!(
            m.catalog_release("dan/demo1", None)
                .unwrap()
                .manifest
                .version,
            "0.2.0"
        );
        let overview = m.browse_catalog().unwrap();
        assert_eq!(overview.format, catalog::CatalogFormat::V2);

        let installed = m.install_from_catalog("dan/demo1", Some("0.1.0")).unwrap();
        assert_eq!(installed.current.version, "0.1.0");
        let rows = m.check().unwrap();
        assert_eq!(rows[0].status, check::CheckStatus::UpdateAvailable);
        assert_eq!(rows[0].latest.as_deref(), Some("0.2.0"));
        m.update("dan/demo1").unwrap();
        assert_eq!(m.check().unwrap()[0].status, check::CheckStatus::UpToDate);
        ws.refuses("catalog", || m.install_from_catalog("dan/missing", None));
        ws.refuses("already_installed", || {
            m.install_from_catalog("dan/demo1", None)
        });
    }

    #[test]
    fn release_with_empty_files_is_still_verified() {
        let ws = Ws::new("dan/demo1");
        let m = ws.open();
        let release = catalog::Release {
            manifest: make_manifest("dan/demo1", "0.1.0"),
            source: ws.author.source().into(),
            commit: ws.author.head(),
            reference: "dan/demo1@0.1.0".into(),
            files: BTreeMap::new(), // claims an empty dist; the commit has dist/index.js
            build: None,
        };
        ws.refuses("catalog", || m.install_release(&release));
    }

    #[test]
    fn ids_differing_only_in_case_are_refused() {
        assert!(ids_collide("dan/p", "Dan/p"));
        assert!(ids_collide("dan/p", "Dan/q"), "namespace folder is shared");
        assert!(ids_collide("dan/p", "dan/P"));
        assert!(!ids_collide("dan/p", "dan/p"));
        assert!(!ids_collide("dan/p", "dan/q"));
        assert!(!ids_collide("dan/p", "other/P"));
        let (ws, m) = Ws::installed("dan/p");
        let upper = ws.other_author("upper", "Dan/p");
        let error = ws.refuses("workspace", || m.install(upper.source()));
        assert!(error.to_string().contains("letter case"), "{error}");
        let sibling = ws.other_author("sibling", "Dan/q");
        ws.refuses("workspace", || m.install(sibling.source()));
        // A forgotten repository still occupies its path.
        m.forget("dan/p").unwrap();
        let error = ws.refuses("workspace", || m.install(upper.source()));
        assert!(error.to_string().contains("letter case"), "{error}");
        m.restore("dan/p").unwrap();
        let error = ws.refuses("invalid_argument", || {
            m.fork_in_namespace("dan/p", &ws.dir.join("forks/p"), "Dan")
        });
        assert!(error.to_string().contains("letter case"), "{error}");
    }

    // ---- fork -----------------------------------------------------------------------

    #[test]
    fn fork_creates_an_independent_author_checkout() {
        let (ws, m) = Ws::installed("vendor/demo1");
        let base = ws.author.head();
        // A branch named `fork` already exists in the installed clone.
        fixture_git(&ws.repo(), &["branch", "fork"]);
        let before = tree_hash(&ws.root);
        let path = m
            .fork_in_namespace("vendor/demo1", &ws.dir.join("forks/demo1"), "me")
            .unwrap();
        assert_eq!(
            tree_hash(&ws.root),
            before,
            "fork never changes the workspace"
        );
        assert_eq!(path, ws.dir.join("forks/demo1"));
        let manifest = crate::model::manifest(&path).unwrap();
        assert_eq!(manifest.id, "me/demo1");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(
            manifest.fork_of,
            Some(ForkOf {
                id: "vendor/demo1".into(),
                version: "0.1.0".into(),
                commit: base.clone()
            })
        );
        assert_eq!(fixture_git(&path, &["remote"]), "upstream");
        assert_eq!(
            fixture_git(&path, &["remote", "get-url", "upstream"]),
            ws.author.source()
        );
        assert_eq!(fixture_git(&path, &["branch", "--show-current"]), "fork-2");
        assert_eq!(fixture_git(&path, &["config", "pm.forkBase"]), base);
        assert_eq!(fixture_git(&path, &["config", "pm.forkVersion"]), "0.1.0");
        assert_eq!(fixture_git(&path, &["status", "--porcelain"]), "");
        assert_eq!(
            fixture_git(&path, &["log", "-1", "--format=%s"]),
            "Fork vendor/demo1 as me/demo1"
        );
        // Installing the fork is independent of the original.
        let r = m
            .install_ref(path.to_str().unwrap(), Some("HEAD"), None)
            .unwrap();
        assert_eq!(r.id, "me/demo1");
        assert_eq!(r.fork_of.unwrap().id, "vendor/demo1");
        assert_eq!(m.list().unwrap().len(), 2);
        // Default namespace.
        let local = m.fork("vendor/demo1", &ws.dir.join("forks/local")).unwrap();
        assert_eq!(crate::model::manifest(&local).unwrap().id, "local/demo1");
    }

    #[test]
    fn fork_refusals_create_nothing() {
        let (ws, m) = Ws::installed("vendor/demo1");
        let id = "vendor/demo1";
        let dest = ws.dir.join("forks/demo1");
        let error = ws.refuses("invalid_argument", || {
            m.fork_in_namespace(id, &dest, "vendor")
        });
        assert!(error.to_string().contains("different namespace"));
        ws.refuses("invalid_id", || m.fork_in_namespace(id, &dest, "bad ns"));
        ws.refuses("invalid_argument", || {
            m.fork_in_namespace(id, &ws.dir.join("forks/../demo1"), "me")
        });
        ws.refuses("invalid_argument", || {
            m.fork_in_namespace(id, &ws.root.join("inside"), "me")
        });
        ws.refuses("invalid_argument", || {
            m.fork_in_namespace(id, &ws.root.join("plugins/new/deeper"), "me")
        });
        fs::create_dir_all(&dest).unwrap();
        let error = ws.refuses("invalid_argument", || m.fork_in_namespace(id, &dest, "me"));
        assert_eq!(error.to_string(), "destination already exists");
        fs::remove_dir(&dest).unwrap();
        assert!(!dest.exists());
        assert!(!ws.root.join("inside").exists());
    }

    // ---- concurrency and known limits -----------------------------------------------

    #[test]
    fn one_handle_serializes_concurrent_operations() {
        let (ws, m) = Ws::installed("dan/demo1");
        let m = std::sync::Arc::new(m);
        let threads: Vec<_> = (0..4)
            .map(|n| {
                let m = m.clone();
                std::thread::spawn(move || {
                    for _ in 0..3 {
                        m.set_enabled("dan/demo1", n % 2 == 0).unwrap();
                        m.uninstall_plan("dan/demo1").unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(m.list().unwrap().len(), 1);
        ws.no_leftovers();
    }

    #[test]
    fn symlink_outside_dist_does_not_block_other_plugins() {
        let (ws, m) = Ws::installed("dan/demo1");
        let linked = ws.other_author("linked", "dan/linked");
        std::os::unix::fs::symlink("plugin.json", linked.path.join("manifest-link")).unwrap();
        fixture_git(&linked.path, &["add", "manifest-link"]);
        fixture_git(
            &linked.path,
            &["commit", "-q", "-m", "symlink outside dist"],
        );
        fixture_git(&linked.path, &["tag", "-f", "dan/linked@0.1.0"]);
        m.install(linked.source()).expect("dist/ has no symlink");
        m.disable("dan/demo1")
            .expect("other plugins stay manageable");
        // The plugin itself stays updatable; its repository symlink is kept.
        linked.release("0.2.0");
        m.update("dan/linked").unwrap();
        assert!(
            fs::symlink_metadata(ws.root.join("plugins/dan/linked/manifest-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        // A symlink inside dist/ is refused before anything is committed.
        let bad = ws.other_author("bad", "dan/bad");
        std::os::unix::fs::symlink("index.js", bad.path.join("dist/link.js")).unwrap();
        fixture_git(&bad.path, &["add", "--all"]);
        fixture_git(&bad.path, &["commit", "-q", "-m", "symlink in dist"]);
        fixture_git(&bad.path, &["tag", "-f", "dan/bad@0.1.0"]);
        ws.refuses("unsupported", || m.install(bad.source()));
    }

    #[test]
    fn symlinked_namespace_directory_is_never_followed() {
        // Install: `plugins/vendor` points outside; nothing may be written there.
        let ws = Ws::new("vendor/demo1");
        let m = ws.open();
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, ws.root.join("plugins/vendor")).unwrap();
        ws.refuses("workspace", || m.install(ws.author.source()));
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

        // Uninstall/update of an installed plugin whose namespace was swapped for a
        // symlink: the external directory survives untouched.
        let (ws, m) = Ws::installed("dan/demo1");
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::rename(ws.root.join("plugins/dan"), outside.join("dan")).unwrap();
        std::os::unix::fs::symlink(outside.join("dan"), ws.root.join("plugins/dan")).unwrap();
        ws.refuses("workspace", || m.uninstall_with("dan/demo1", true, true));
        assert!(outside.join("dan/demo1/.git").is_dir());
        ws.author.release("0.2.0");
        ws.refuses("workspace", || m.update("dan/demo1"));

        let (ws, m) = Ws::installed("dan/demo1");
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::rename(ws.root.join("dist/dan"), outside.join("dan")).unwrap();
        std::os::unix::fs::symlink(outside.join("dan"), ws.root.join("dist/dan")).unwrap();
        ws.refuses("workspace", || m.uninstall_with("dan/demo1", true, true));
        assert!(outside.join("dan/demo1").is_dir());
    }

    #[test]
    fn update_and_rollback_refuse_hidden_local_work() {
        type Setup = fn(&Path);
        let cases: [(&str, Setup); 3] = [
            ("stash", |repo| {
                fs::write(repo.join("plugin.json"), "{}\n").unwrap();
                fixture_git(repo, &["stash", "-q"]);
            }),
            ("branch", |repo| {
                fixture_git(repo, &["switch", "-q", "-c", "local-only"]);
                fs::write(repo.join("extra.txt"), "x\n").unwrap();
                fixture_git(repo, &["add", "extra.txt"]);
                fixture_git(repo, &["commit", "-q", "-m", "branch only"]);
                fixture_git(repo, &["switch", "-q", "--detach", "HEAD~1"]);
            }),
            ("tag", |repo| {
                fixture_git(repo, &["switch", "-q", "--detach"]);
                fs::write(repo.join("extra.txt"), "x\n").unwrap();
                fixture_git(repo, &["add", "extra.txt"]);
                fixture_git(repo, &["commit", "-q", "-m", "tag only"]);
                fixture_git(repo, &["tag", "local-only"]);
                fixture_git(repo, &["switch", "-q", "--detach", "HEAD~1"]);
            }),
        ];
        for (name, setup) in cases {
            let (ws, m) = Ws::installed("dan/demo1");
            ws.author.release("0.2.0");
            m.update("dan/demo1").unwrap();
            ws.author.release("0.3.0");
            setup(&ws.repo());
            let error = ws.refuses("local_changes", || m.update("dan/demo1"));
            assert!(error.to_string().contains("stash"), "{name}: {error}");
            ws.refuses("local_changes", || m.rollback("dan/demo1"));
        }
    }

    #[test]
    fn symlinked_workspace_root_shares_the_lock() {
        let ws = Ws::new("dan/demo1");
        let m = ws.open();
        let alias = ws.dir.join("alias");
        std::os::unix::fs::symlink(&ws.root, &alias).unwrap();
        let error = Manager::open(&alias)
            .err()
            .expect("same workspace, same lock");
        assert_eq!(error.kind(), "locked");
        drop(m);
    }
}
