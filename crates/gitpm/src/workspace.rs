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
    model::{
        manifest, manifest_json, package_manifest, resolve_ref, resolve_upstream_ref, revision,
    },
    packages, releases, scaffold,
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

mod clones;
mod fork;
use clones::{
    configured_origin, guard_clone, installed_repo, place_clone, plugin_repo, removal, settle_head,
    shared_head_moved,
};
pub use fork::{
    DependencyRewrite, ForkReplacement, ForkStep, ForkedPackage, PackageFork,
    PackageForkReplacement,
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
    /// Metadata keys of the installed manifest with the wrong type, ignored (see
    /// [`metadata_warnings`]); the caller should show them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
/// Result of update/rollback: `from` → `to`; `changed` is false for a no-op update.
#[derive(Clone, Debug, Serialize)]
pub struct RevisionChange {
    pub id: String,
    pub from: Revision,
    pub to: Revision,
    pub changed: bool,
    /// Metadata keys of the new manifest with the wrong type, ignored.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
/// `ID VERSION: plugin.json KEY must be …; ignored` for each wrong-typed metadata key.
/// Installing is lenient about metadata (only authoring refuses it).
pub(crate) fn metadata_warnings(m: &Manifest) -> Vec<String> {
    m.metadata_problems()
        .into_iter()
        .map(|problem| format!("{} {}: plugin.json {problem}; ignored", m.id, m.version))
        .collect()
}
/// Joint stable-release upgrade plan. No plugins are removed or downgraded.
#[derive(Clone, Debug, Default, Serialize)]
pub struct UpdateAllPlan {
    pub updates: Vec<RevisionChange>,
    pub installs: Vec<InstallResult>,
    pub kept_back: Vec<HeldBack>,
}
/// A newer release exists, but the chosen compatible solution uses an older one.
#[derive(Clone, Debug, Serialize)]
pub struct HeldBack {
    pub id: String,
    pub selected: String,
    pub latest: String,
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
    /// The workspace state (`state.json`, any readable format; see [`crate::STATE_FORMAT`]).
    pub fn state(&self) -> Result<State> {
        State::parse(&fs::read(self.root.join("state.json"))?)
    }
    /// Installed plugins as a flat list (sorted by ID).
    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        Ok(self.state()?.installed_in(&self.root))
    }
    /// Run `action` on a transaction that stages only the paths it touches; nothing
    /// in the live workspace changes unless the action succeeds.
    fn transaction<T>(&self, action: impl FnOnce(&mut Txn, &mut State) -> Result<T>) -> Result<T> {
        let _busy = self.busy();
        let mut txn = Txn::begin(&self.root)?;
        let mut state = self.state()?;
        let value = action(&mut txn, &mut state)?;
        // The installed revisions of each shared clone stay reachable (see `clones`): in
        // the staged copy as part of the transaction, else in the live clone afterwards.
        let keys: std::collections::BTreeSet<&str> = state
            .plugins
            .values()
            .chain(state.forgotten.values())
            .filter_map(|p| p.clone.as_deref())
            .collect();
        let mut live = Vec::new();
        for key in keys {
            match txn.staged_repo(&format!("repos/{key}")) {
                Some(staged) => clones::sync_refs(&staged, &state, key)?,
                None => live.push(key),
            }
        }
        txn.commit(&state)?;
        for key in live {
            let repo = self.root.join("repos").join(key);
            if repo.join(".git").is_dir() {
                // Best effort: the change is committed; the next fetch syncs again.
                let _ = clones::sync_refs(&repo, &state, key);
            }
        }
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
                package: expected.map(|m| m.id.as_str()),
                path: None,
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
        self.install_package(source, None, reference, dev)
    }
    /// [`Manager::install_channel`] of one package of the repository. A repository with a
    /// `gitpm.json` listing several packages needs `package` (unless `reference` is an
    /// `ID@VERSION` tag); for a single-package repository a given `package` must match.
    pub fn install_package(
        &self,
        source: &str,
        package: Option<&str>,
        reference: Option<&str>,
        dev: bool,
    ) -> Result<InstallResult> {
        ensure!(
            !dev || reference.is_none(),
            InvalidArgument,
            "the dev channel conflicts with an explicit ref"
        );
        if let Some(package) = package {
            valid_id(package)?;
        }
        self.install_checked(
            Fetch {
                source,
                reference: if dev { Some("HEAD") } else { reference },
                expected: None,
                files: None,
                package,
                path: None,
            },
            None,
        )
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
            let cloned = clone_revision(txn, &fetch, "incoming")?;
            let m = cloned.manifest.clone();
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
            let skipped = cloned.skipped.clone();
            let mut result = adopt(txn, state, cloned, source.clone(), origin.clone())?;
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
            let cloned = clone_revision(
                txn,
                &Fetch::release(&release),
                &format!("dependency-{number}"),
            )?;
            check_new_id(txn.root(), state, &cloned.manifest)?;
            adopt(
                txn,
                state,
                cloned,
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
    /// Resolve all installed plugins together, without changing installed files or state.
    /// Reads release tags from each origin; new dependencies come from the catalog.
    /// Prefers newer versions, with installed IDs in lexical order as tie-breaker.
    pub fn update_all_plan(&self) -> Result<UpdateAllPlan> {
        let _busy = self.busy();
        let mut txn = Txn::begin(&self.root)?;
        let mut state = self.state()?;
        self.prepare_update_all(&mut txn, &mut state)
    }

    /// Resolve and apply a joint upgrade in one recoverable transaction.
    /// Local work blocks the operation. No merge, deletion or downgrade is attempted.
    pub fn update_all(&self) -> Result<UpdateAllPlan> {
        self.transaction(|txn, state| self.prepare_update_all(txn, state))
    }

    fn prepare_update_all(&self, txn: &mut Txn, state: &mut State) -> Result<UpdateAllPlan> {
        let mut candidates = crate::upgrade::Candidates::new();
        let mut latest = BTreeMap::new();
        // One scratch copy per clone: the packages of a multi-package source share it.
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, installed) in &state.plugins {
            groups
                .entry(installed.clone_folder(id))
                .or_default()
                .push(id.clone());
        }
        let mut repos: BTreeMap<String, PathBuf> = BTreeMap::new();
        for (number, (relative, ids)) in groups.iter().enumerate() {
            let first = &ids[0];
            let key = state.plugins[first].clone.as_deref();
            let live = installed_repo(txn.root(), first, &state.plugins[first])?;
            guard_clone(
                &live,
                state,
                first,
                key,
                "local commits detected; update would discard them",
                "unpublished local work blocks update all",
                true,
            )?;
            // Scratch copies ensure even held-back repositories keep their original refs.
            let repo = txn.scratch(&format!("upgrade-{number}"));
            copy_tree(&live, &repo, true)?;
            let mut known = Vec::new();
            for id in ids {
                known.extend(releases::discover(&repo, Some(id))?);
            }
            match key {
                // The registered packages of the group are checked below.
                Some(key) => {
                    let except: Vec<&str> = ids.iter().map(String::as_str).collect();
                    clones::fetch_tags(&repo, state, key, &except, true)?;
                }
                None => {
                    git(
                        &repo,
                        &[
                            "fetch",
                            "origin",
                            "--prune",
                            "--prune-tags",
                            "--force",
                            "--tags",
                        ],
                    )?;
                }
            }
            for tag in known {
                if git(&repo, &["tag", "--list", &tag.name])? == tag.name {
                    ensure!(
                        resolve_ref(&repo, &format!("refs/tags/{}", tag.name))? == tag.commit,
                        VersionConflict,
                        "release tag {} changed commit",
                        tag.name
                    );
                }
            }
            for id in ids {
                let installed = &state.plugins[id];
                let current = deps::version(&installed.current.version)?;
                let mut newest = current.clone();
                let mut versions = BTreeMap::new();
                // Read at the installed commit, from the recorded package folder: a shared
                // clone's HEAD may be another package's revision, and a root package may
                // have moved into a package folder since.
                let current_manifest =
                    packages::find_at(&repo, &installed.current.commit, Some(id))?.manifest;
                versions.insert(
                    current.clone(),
                    catalog::Release {
                        manifest: current_manifest,
                        source: installed.source.clone(),
                        commit: installed.current.commit.clone(),
                        reference: installed.current.commit.clone(),
                        files: BTreeMap::new(),
                        build: None,
                        path: installed.path.clone(),
                    },
                );
                for tag in releases::discover(&repo, Some(id))? {
                    let version = deps::version(&tag.manifest.version)?;
                    if version < current {
                        continue;
                    }
                    ensure!(
                        version != current || tag.commit == installed.current.commit,
                        VersionConflict,
                        "release commit changed for installed version of {id}"
                    );
                    newest = newest.max(version.clone());
                    // An active replacement must continue providing the same original ID.
                    if state.replacements.iter().any(|(original, fork)| {
                        fork == id
                            && tag
                                .manifest
                                .replaces
                                .as_ref()
                                .is_none_or(|r| r.id != *original)
                    }) {
                        continue;
                    }
                    deps::validate(&tag.manifest)?;
                    versions.insert(
                        version,
                        catalog::Release {
                            manifest: tag.manifest,
                            source: installed.source.clone(),
                            commit: tag.commit,
                            reference: tag.name,
                            files: BTreeMap::new(),
                            build: None,
                            path: (!tag.path.is_empty()).then_some(tag.path),
                        },
                    );
                }
                latest.insert(id.clone(), newest.to_string());
                candidates.insert(id.clone(), versions);
            }
            repos.insert(relative.clone(), repo);
        }
        let selected = crate::upgrade::solve(state, candidates, self.options.catalog.as_deref())?;
        let mut plan = UpdateAllPlan::default();
        for (number, (id, release)) in selected.iter().enumerate() {
            let m = &release.manifest;
            if let Some(installed) = state.plugins.get(id) {
                if latest[id] != m.version {
                    plan.kept_back.push(HeldBack {
                        id: id.clone(),
                        selected: m.version.clone(),
                        latest: latest[id].clone(),
                    });
                }
                if release.commit == installed.current.commit {
                    continue;
                }
                let relative = installed.clone_folder(id);
                let repo = match txn.staged_repo(&relative) {
                    Some(staged) => staged,
                    None => repos[&relative].clone(),
                };
                git(&repo, &["checkout", "--detach", &release.commit])?;
                let actual = package_manifest(&repo, release.path.as_deref())?;
                ensure!(
                    serde_json::to_value(&actual)? == serde_json::to_value(m)?,
                    VersionConflict,
                    "release manifest changed for {id}"
                );
                let next = revision(&repo, m)?;
                let installed = state.plugins.get_mut(id).unwrap();
                plan.updates.push(RevisionChange {
                    id: id.clone(),
                    from: installed.current.clone(),
                    to: next.clone(),
                    changed: true,
                    warnings: metadata_warnings(m),
                });
                installed.previous = Some(installed.current.clone());
                installed.current = next;
                installed.path = release.path.clone();
                installed.adopt(m);
                let path = installed.path.clone();
                if txn.staged_repo(&relative).is_none() {
                    txn.add_repo(&relative, &repo)?;
                }
                txn.deploy(id, &relative, path.as_deref())?;
            } else {
                let origin = catalog::canonical_location(self.catalog_location()?)?;
                ensure_single_catalog(state, &origin)?;
                let cloned = clone_revision(
                    txn,
                    &Fetch::release(release),
                    &format!("upgrade-new-{number}"),
                )?;
                check_new_id(txn.root(), state, &cloned.manifest)?;
                plan.installs.push(adopt(
                    txn,
                    state,
                    cloned,
                    release.source.clone(),
                    Some(origin),
                )?);
            }
        }
        Ok(plan)
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
            "the dev channel conflicts with an explicit ref"
        );
        valid_id(id)?;
        self.transaction(|txn, state| {
            let p = state
                .plugins
                .get(id)
                .cloned()
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
            let relative = p.clone_folder(id);
            let live = installed_repo(txn.root(), id, &p)?;
            guard_clone(
                &live,
                state,
                id,
                p.clone.as_deref(),
                "local commits detected; update would discard them",
                "local branches, tags or stash hold unpublished work; keep or remove them before updating",
                true,
            )?;
            let repo = txn.repo(&relative, &live)?;
            let known_releases = match releases::discover(&repo, Some(id)) {
                Ok(tags) => tags,
                Err(Error::NoReleases(_)) => Vec::new(),
                Err(error) => return Err(error),
            };
            // Mirror upstream tags exactly: a deleted release disappears, a moved one is
            // seen at its new commit (and refused below as a version conflict). Local
            // tags holding unpublished commits were refused above, so nothing is lost.
            match p.clone.as_deref() {
                Some(key) => clones::fetch_tags(&repo, state, key, &[id], true)?,
                None => {
                    git(
                        &repo,
                        &["fetch", "origin", "--prune", "--prune-tags", "--force", "--tags"],
                    )?;
                }
            }
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
                    "latest release is older than installed version; downgrading requires an explicit ref"
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
                    warnings: Vec::new(),
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
            // The package can move to another folder (or into a multi-package layout).
            let path = match packages::find_at(&repo, &target, Some(id)) {
                Ok(found) => found.subdir().map(str::to_owned),
                Err(Error::InvalidArgument(_)) => {
                    fail!(InvalidManifest, "plugin id changed upstream")
                }
                Err(error) => return Err(error),
            };
            git(&repo, &["checkout", "--detach", &target])?;
            let m = package_manifest(&repo, path.as_deref())?;
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
            entry.path = path.clone();
            entry.adopt(&m);
            txn.deploy(id, &relative, path.as_deref())?;
            Ok(RevisionChange {
                id: id.into(),
                from: p.current,
                to: next,
                changed: true,
                warnings: metadata_warnings(&m),
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
            let relative = p.clone_folder(id);
            let live = installed_repo(txn.root(), id, &p)?;
            guard_clone(
                &live,
                state,
                id,
                p.clone.as_deref(),
                "local commits detected",
                "local branches, tags or stash hold unpublished work; keep or remove them before rolling back",
                false,
            )?;
            let repo = txn.repo(&relative, &live)?;
            let path = match packages::find_at(&repo, &previous.commit, Some(id)) {
                Ok(found) => found.subdir().map(str::to_owned),
                Err(Error::InvalidArgument(_)) => fail!(InvalidManifest, "rollback manifest mismatch"),
                Err(error) => return Err(error),
            };
            git(&repo, &["checkout", "--detach", &previous.commit])?;
            let m = package_manifest(&repo, path.as_deref())?;
            ensure!(
                m.id == id && m.version == previous.version,
                InvalidManifest,
                "rollback manifest mismatch"
            );
            self.check_change(txn, state, id, &m, false)?;
            let entry = state.plugins.get_mut(id).unwrap();
            entry.previous = Some(p.current.clone());
            entry.current = previous.clone();
            entry.path = path.clone();
            entry.adopt(&m);
            txn.deploy(id, &relative, path.as_deref())?;
            Ok(RevisionChange {
                id: id.into(),
                from: p.current,
                to: previous,
                changed: true,
                warnings: Vec::new(),
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
    /// plugin provides are dropped (their consumers go back to the original). A package
    /// of a shared clone is kept in `State::forgotten`, so its clone stays while it is
    /// forgotten and `restore`/`uninstall` find it; the clone's HEAD moves to a
    /// registered package's revision if it was this package's.
    pub fn forget_with(&self, id: &str, force: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            if !state.plugins.contains_key(id) {
                return Err(Error::NotInstalled { id: id.into() });
            }
            if !force {
                refuse_dependents(state, id, "forget")?;
            }
            let installed = state.plugins.remove(id).unwrap();
            state.replacements.retain(|_, fork| fork != id);
            if let Some(key) = installed.clone.clone() {
                let relative = installed.clone_folder(id);
                state.forgotten.insert(id.to_owned(), installed);
                settle_head(txn, state, &key, id, &relative)?;
            }
            Ok(())
        })
    }
    /// Re-register a retained (forgotten) clean repository; previous history is not
    /// restored. An own clone is registered at its HEAD; a forgotten package of a shared
    /// clone at its recorded revision (the clone is checked out there if needed).
    pub fn restore(&self, id: &str) -> Result<InstallResult> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            if let Some(record) = state.forgotten.get(id).cloned() {
                return restore_shared(txn, state, id, record);
            }
            // A missing clone is the real problem even when the ID is still registered.
            let repo = plugin_repo(txn.root(), id)?;
            if state.plugins.contains_key(id) {
                return Err(Error::AlreadyInstalled {
                    id: id.into(),
                    message: format!("plugin {id} is already registered"),
                });
            }
            clean(&repo, id)?;
            // The package may have moved into a folder of a multi-package layout.
            let head = git(&repo, &["rev-parse", "HEAD"])?;
            let path = match packages::find_at(&repo, &head, Some(id)) {
                Ok(found) => found.subdir().map(str::to_owned),
                Err(Error::InvalidArgument(_)) => fail!(InvalidManifest, "plugin id mismatch"),
                Err(error) => return Err(error),
            };
            let m = package_manifest(&repo, path.as_deref())?;
            ensure!(m.id == id, InvalidManifest, "plugin id mismatch");
            let mut installed = Installed {
                source: configured_origin(&repo)
                    .require(Error::Git, "the retained clone has no origin remote")?,
                current: revision(&repo, &m)?,
                previous: None,
                enabled: true,
                path: path.clone(),
                ..Default::default()
            };
            installed.adopt(&m);
            let result = InstallResult {
                id: id.into(),
                source: installed.source.clone(),
                current: installed.current.clone(),
                fork_of: installed.fork_of.clone(),
                skipped: Vec::new(),
                warnings: Vec::new(),
            };
            state.plugins.insert(id.to_owned(), installed);
            txn.deploy(id, &format!("plugins/{id}"), path.as_deref())?;
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
        let removal = removal(&self.root, &state, id)?;
        let mut deletes = Vec::new();
        if let Some(repo) = &removal.repo {
            deletes.push(repo.to_string_lossy().into_owned());
        }
        let dist = self.root.join("dist").join(id);
        if dist.exists() {
            deletes.push(dist.to_string_lossy().into_owned());
        }
        Ok(UninstallPlan {
            id: id.into(),
            registered: state.plugins.contains_key(id),
            deletes,
            lost_work: removal.lost_work,
            dependents: deps::dependents(&state, id),
        })
    }
    /// Remove state entry, repository and dist; for a package of a shared clone the clone
    /// stays while other packages (registered or forgotten) use it, and its HEAD moves to
    /// one of theirs. A forgotten package is uninstalled the same way (its `dist/`, its
    /// record and the clone if it was the last user). With `confirm == false` any local work
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
            let removal = removal(txn.root(), state, id)?;
            if !force {
                refuse_dependents(state, id, "uninstall")?;
            }
            if !confirm && !removal.lost_work.is_empty() {
                return Err(lost_work::refusal(id, removal.lost_work));
            }
            state.plugins.remove(id);
            state.forgotten.remove(id);
            state.replacements.retain(|_, fork| fork != id);
            match (&removal.repo, &removal.key) {
                (Some(_), _) => txn.delete_repo(&removal.relative)?,
                // Other packages keep the shared clone; its HEAD goes to one of theirs.
                (None, Some(key)) => settle_head(txn, state, key, id, &removal.relative)?,
                (None, None) => {}
            }
            txn.delete_dist(id)?;
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

/// [`Manager::restore`] of a forgotten package of a shared clone: register it again at
/// its recorded revision and deploy that revision's `dist/`.
fn restore_shared(
    txn: &mut Txn,
    state: &mut State,
    id: &str,
    record: Installed,
) -> Result<InstallResult> {
    let key = record.clone.clone().unwrap_or_default();
    let relative = record.clone_folder(id);
    let live = match installed_repo(txn.root(), id, &record) {
        Err(Error::RepositoryNotFound { .. }) => fail!(
            Workspace,
            "the shared clone {relative} of the forgotten {id} is gone; uninstall {id} to drop its record, then install it again"
        ),
        live => live?,
    };
    clean(&live, id)?;
    let commit = record.current.commit.clone();
    if git(&live, &["rev-parse", "HEAD"])? != commit {
        if let Some(item) = shared_head_moved(&live, state, &key, id)? {
            return Err(Error::LocalChanges {
                id: id.into(),
                message: format!("the shared clone {relative} has local commits"),
                items: vec![item],
            });
        }
        let staged = txn.repo(&relative, &live)?;
        git(&staged, &["checkout", "-q", "--detach", &commit])?;
    }
    let repo = txn.staged_repo(&relative).unwrap_or(live);
    let path = packages::find_at(&repo, &commit, Some(id))?
        .subdir()
        .map(str::to_owned);
    let m = package_manifest(&repo, path.as_deref())?;
    ensure!(
        m.id == id && m.version == record.current.version,
        InvalidManifest,
        "the recorded revision of {id} has another manifest"
    );
    let mut installed = Installed {
        previous: None,
        enabled: true,
        path: path.clone(),
        ..record
    };
    installed.adopt(&m);
    let result = InstallResult {
        id: id.into(),
        source: installed.source.clone(),
        current: installed.current.clone(),
        fork_of: installed.fork_of.clone(),
        skipped: Vec::new(),
        warnings: Vec::new(),
    };
    state.forgotten.remove(id);
    state.plugins.insert(id.to_owned(), installed);
    txn.deploy(id, &relative, path.as_deref())?;
    Ok(result)
}

/// Where an install takes its revision from.
#[derive(Clone, Copy)]
struct Fetch<'a> {
    source: &'a str,
    reference: Option<&'a str>,
    expected: Option<&'a Manifest>,
    files: Option<&'a BTreeMap<String, catalog::FileDigest>>,
    /// Package ID to pick in a multi-package repository (checked against a root package).
    package: Option<&'a str>,
    /// Exact package folder the catalog recorded (`None`: the root); only with `expected`.
    path: Option<&'a str>,
}
impl<'a> Fetch<'a> {
    fn release(release: &'a catalog::Release) -> Self {
        Fetch {
            source: &release.source,
            reference: Some(&release.commit),
            expected: Some(&release.manifest),
            files: (!release.files.is_empty()).then_some(&release.files),
            package: Some(&release.manifest.id),
            path: release.path.as_deref(),
        }
    }
}

/// A verified scratch clone checked out at the revision to install.
struct Cloned {
    repo: PathBuf,
    manifest: Manifest,
    /// Package folder (`None`: the repository root).
    path: Option<String>,
    skipped: Vec<releases::SkippedTag>,
}

/// Clone into the transaction's scratch area, check out and verify the revision.
fn clone_revision(txn: &Txn, fetch: &Fetch, name: &str) -> Result<Cloned> {
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
            let mut found = match releases::discover_all(&repo, fetch.package) {
                // An ID the repository does not have: say which packages it has.
                Err(Error::NoReleases(message)) => {
                    if let Some(id) = fetch.package {
                        let head = git(&repo, &["rev-parse", "HEAD"])?;
                        packages::find_at(&repo, &head, Some(id))?;
                    }
                    return Err(Error::NoReleases(message));
                }
                other => other?,
            };
            found.ensure_latest_is_valid()?;
            skipped = std::mem::take(&mut found.skipped);
            found
                .tags
                .pop()
                .require(Error::NoReleases, "no releases")?
                .commit
        }
    };
    // An `ID@VERSION` tag names its package.
    let package = fetch
        .package
        .or_else(|| fetch.reference.and_then(packages::tag_id));
    let found = packages::find_at(&repo, &commit, package)?;
    if fetch.expected.is_some() {
        ensure!(
            found.subdir() == fetch.path,
            Catalog,
            "catalog package folder {} does not match git revision ({})",
            fetch.path.unwrap_or("(root)"),
            found.subdir().unwrap_or("(root)")
        );
    }
    git(&repo, &["checkout", "--detach", &commit])?;
    let path = found.subdir().map(str::to_owned);
    let m = package_manifest(&repo, path.as_deref())?;
    ensure!(
        m.id == found.manifest.id,
        InvalidManifest,
        "package manifest changed during checkout"
    );
    if let Some(expected) = fetch.expected {
        ensure!(
            serde_json::to_value(&m)? == serde_json::to_value(expected)?,
            Catalog,
            "catalog manifest does not match git revision"
        );
    }
    if let Some(expected_files) = fetch.files {
        let dist = match &path {
            Some(path) => repo.join(path).join("dist"),
            None => repo.join("dist"),
        };
        ensure!(
            catalog::hash_files(&dist)? == *expected_files,
            Catalog,
            "catalog file hashes do not match git dist"
        );
    }
    Ok(Cloned {
        repo,
        manifest: m,
        path,
        skipped,
    })
}

/// The new plugin's ID must not collide with anything installed or retained.
fn check_new_id(root: &Path, state: &State, m: &Manifest) -> Result<()> {
    if state.plugins.contains_key(&m.id) {
        return Err(Error::AlreadyInstalled {
            message: format!("already installed: {}", m.id),
            id: m.id.clone(),
        });
    }
    if state.forgotten.contains_key(&m.id) {
        return Err(Error::AlreadyInstalled {
            message: format!(
                "{} was forgotten and its files are kept; use restore, or uninstall it first",
                m.id
            ),
            id: m.id.clone(),
        });
    }
    let taken = || state.plugins.keys().chain(state.forgotten.keys());
    // A legacy ID can be a prefix of a scoped ID; never nest a repository in another.
    ensure!(
        !taken()
            .any(|id| id.starts_with(&format!("{}/", m.id)) || m.id.starts_with(&format!("{id}/"))),
        Workspace,
        "plugin ID conflicts with an installed namespace path"
    );
    if let Some(other) = taken().find(|id| ids_collide(id, &m.id)) {
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
    cloned: Cloned,
    source: String,
    catalog: Option<String>,
) -> Result<InstallResult> {
    let (staged, relative, key) = place_clone(txn, state, &cloned, &source)?;
    let m = &cloned.manifest;
    let current = revision(&staged, m)?;
    txn.deploy(&m.id, &relative, cloned.path.as_deref())?;
    let mut installed = Installed {
        source: source.clone(),
        current: current.clone(),
        previous: None,
        enabled: true,
        catalog,
        clone: key,
        path: cloned.path.clone(),
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
        warnings: metadata_warnings(m),
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
            "cannot {action} {id}: required by {}; doing it anyway requires force",
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
        assert_eq!(
            read(&root.join("state.json")),
            "{\n  \"format\": 2,\n  \"plugins\": {}\n}"
        );
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
    fn update_all_recovers_every_plugin_after_an_interrupted_commit() {
        // Two repositories, two payloads and state.json: nine journal renames.
        for renames in 0..9 {
            let (ws, m) = Ws::installed("x/a");
            let other = ws.other_author("other", "x/b");
            m.install(other.source()).unwrap();
            ws.author.release("0.2.0");
            other.release("0.2.0");
            crash_after(renames, || m.update_all());
            drop(m);
            let recovered = ws.open();
            for author in [&ws.author, &other] {
                let state = recovered.state().unwrap();
                let installed = &state.plugins[&author.id];
                assert_eq!(installed.current.version, "0.2.0", "crash after {renames}");
                assert_eq!(installed.previous.as_ref().unwrap().version, "0.1.0");
                assert_eq!(installed.current.commit, author.head());
                assert_eq!(
                    tree_hash(&ws.root.join("dist").join(&author.id)),
                    tree_hash(&author.path.join("dist"))
                );
            }
            ws.no_leftovers();
        }
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
            path: None,
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
            path: None,
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
        assert!(
            error
                .to_string()
                .starts_with("destination already exists and is not a multi-package repository"),
            "{error}"
        );
        fs::remove_dir(&dest).unwrap();
        assert!(!dest.exists());
        assert!(!ws.root.join("inside").exists());
        // Not inside the source's working tree either (new or existing folder).
        let before = tree_hash(&ws.author.path);
        for inside in ["forks/demo1", "."] {
            let error = ws.refuses("invalid_argument", || {
                m.fork_in_namespace(id, &ws.author.path.join(inside), "me")
            });
            assert!(error.to_string().contains("source repository"), "{error}");
        }
        assert_eq!(tree_hash(&ws.author.path), before);
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

    // ---- multi-package repositories ------------------------------------------------

    /// Author repository with `gitpm.json` and packages under `packages/`.
    struct Mono {
        dir: TempDir,
        repo: PathBuf,
        root: PathBuf,
    }
    impl Mono {
        /// `tilr/store` and `tilr/theme` at 1.0.0, each tagged on the same commit.
        fn new() -> Self {
            let dir = TempDir::new("mono");
            let repo = dir.join("mono");
            fs::create_dir_all(&repo).unwrap();
            fixture_git(&repo, &["init", "-q", "-b", "main"]);
            fs::write(
                repo.join(packages::CONFIG),
                r#"{"packages":["packages/*"]}"#,
            )
            .unwrap();
            let mono = Self {
                root: dir.join("workspace"),
                repo,
                dir,
            };
            mono.write("store", "1.0.0", &[]);
            mono.write("theme", "1.0.0", &[]);
            mono.commit("first");
            mono.tag("store", "1.0.0");
            mono.tag("theme", "1.0.0");
            mono
        }
        fn write(&self, name: &str, version: &str, deps: &[(&str, &str)]) {
            let folder = self.repo.join("packages").join(name);
            fs::create_dir_all(folder.join("dist")).unwrap();
            let mut m = make_manifest(&format!("tilr/{name}"), version);
            for (id, requirement) in deps {
                m.dependencies.insert((*id).into(), (*requirement).into());
            }
            write_manifest(&folder, &m);
            fs::write(folder.join("dist/index.js"), format!("{name} {version}")).unwrap();
        }
        fn commit(&self, message: &str) -> String {
            fixture_git(&self.repo, &["add", "--all"]);
            fixture_git(&self.repo, &["commit", "-q", "-m", message]);
            fixture_git(&self.repo, &["rev-parse", "HEAD"])
        }
        fn tag(&self, name: &str, version: &str) {
            fixture_git(&self.repo, &["tag", &format!("tilr/{name}@{version}")]);
        }
        fn release(&self, name: &str, version: &str) -> String {
            self.write(name, version, &[]);
            let commit = self.commit(&format!("{name} {version}"));
            self.tag(name, version);
            commit
        }
        fn source(&self) -> &str {
            self.repo.to_str().unwrap()
        }
        fn open(&self) -> Manager {
            reopen(&self.root).unwrap()
        }
        fn dist(&self, name: &str) -> String {
            read(&self.root.join("dist/tilr").join(name).join("index.js"))
        }
        fn clones(&self) -> Vec<String> {
            fs::read_dir(self.root.join("repos"))
                .map(|entries| {
                    entries
                        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        }
        fn refuses<T: Debug>(&self, kind: &str, op: impl FnOnce() -> Result<T>) -> Error {
            let before = tree_hash(&self.root);
            let error = op().expect_err("operation must fail");
            assert_eq!(error.kind(), kind, "{error}");
            assert_eq!(tree_hash(&self.root), before, "workspace changed: {error}");
            assert!(!self.root.join(txn::DIR).exists());
            error
        }
    }

    #[test]
    fn packages_of_one_source_share_one_clone() {
        let mono = Mono::new();
        let m = mono.open();
        let store = m
            .install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        assert_eq!(store.current.version, "1.0.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        assert_eq!(mono.dist("store"), "store 1.0.0");
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
        let clones = mono.clones();
        assert_eq!(clones.len(), 1, "{clones:?}");
        assert!(clones[0].starts_with("mono-"), "{clones:?}");
        assert!(!mono.root.join("plugins/tilr").exists());
        let state = m.state().unwrap();
        let p = &state.plugins["tilr/store"];
        assert_eq!(p.clone.as_deref(), Some(clones[0].as_str()));
        assert_eq!(p.path.as_deref(), Some("packages/store"));
        // Only the package's own dist is deployed.
        assert!(!mono.root.join("dist/tilr/store/plugin.json").exists());
        let listed = m.list().unwrap();
        assert_eq!(listed[1].path.as_deref(), Some("packages/theme"));
    }

    #[test]
    fn a_monorepo_install_needs_a_package_id() {
        let mono = Mono::new();
        let m = mono.open();
        let error = mono.refuses("invalid_argument", || m.install(mono.source()));
        let text = error.to_string();
        assert!(
            text.contains("tilr/store (packages/store)")
                && text.contains("tilr/theme (packages/theme)"),
            "{text}"
        );
        mono.refuses("invalid_argument", || {
            m.install_package(mono.source(), Some("tilr/missing"), None, false)
        });
        // An ID@VERSION ref names the package.
        m.install_ref(mono.source(), Some("tilr/theme@1.0.0"), None)
            .unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
        // A root repository checks a given ID.
        let ws = Ws::new("dan/demo1");
        let other = ws.open();
        ws.refuses("invalid_argument", || {
            other.install_package(ws.author.source(), Some("dan/other"), None, false)
        });
        other
            .install_package(ws.author.source(), Some("dan/demo1"), None, false)
            .unwrap();
    }

    #[test]
    fn packages_release_and_update_independently() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        mono.release("store", "1.1.0");
        let change = m.update("tilr/store").unwrap();
        assert_eq!(
            (change.to.version.as_str(), change.changed),
            ("1.1.0", true)
        );
        assert_eq!(mono.dist("store"), "store 1.1.0");
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
        // The other package has no new release: nothing changes.
        assert!(!m.update("tilr/theme").unwrap().changed);
        let state = m.state().unwrap();
        assert_eq!(state.plugins["tilr/theme"].current.version, "1.0.0");
        m.rollback("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.0.0");
        m.update("tilr/store").unwrap();

        // Joint upgrade moves both packages in one clone.
        mono.release("store", "1.2.0");
        mono.release("theme", "2.0.0");
        let preview = m.update_all_plan().unwrap();
        assert_eq!(preview.updates.len(), 2);
        let plan = m.update_all().unwrap();
        assert_eq!(plan.updates.len(), 2);
        assert_eq!(mono.dist("store"), "store 1.2.0");
        assert_eq!(mono.dist("theme"), "theme 2.0.0");
        assert_eq!(mono.clones().len(), 1);

        // The dev channel follows the default branch for one package.
        mono.write("theme", "2.1.0", &[]);
        let head = mono.commit("theme work");
        let dev = m.update_channel("tilr/theme", None, true).unwrap();
        assert_eq!(dev.to.commit, head);
        assert_eq!(mono.dist("theme"), "theme 2.1.0");
        assert_eq!(mono.dist("store"), "store 1.2.0");
        // HEAD of the shared clone is now theme's untagged revision: store still updates.
        mono.release("store", "1.3.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.3.0");
    }

    #[test]
    fn uninstalling_one_package_keeps_the_shared_clone() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert_eq!(plan.deletes.len(), 1, "{:?}", plan.deletes);
        assert!(plan.deletes[0].ends_with("dist/tilr/store"));
        m.uninstall("tilr/store", false).unwrap();
        assert!(!mono.root.join("dist/tilr/store").exists());
        assert_eq!(mono.clones().len(), 1);
        mono.release("theme", "1.1.0");
        m.update("tilr/theme").unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.1.0");
        // Reinstall reuses the clone.
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        assert_eq!(mono.clones().len(), 1);
        m.uninstall("tilr/store", false).unwrap();
        // The last package takes the clone with it; local work there is guarded.
        let clone = mono.root.join("repos").join(&mono.clones()[0]);
        fs::write(clone.join("scratch.txt"), "mine").unwrap();
        mono.refuses("local_changes", || m.update("tilr/theme"));
        let plan = m.uninstall_plan("tilr/theme").unwrap();
        assert_eq!(plan.deletes.len(), 2);
        assert!(!plan.lost_work.is_empty());
        mono.refuses("local_changes", || m.uninstall("tilr/theme", false));
        m.uninstall("tilr/theme", true).unwrap();
        assert!(mono.clones().is_empty());
        assert!(m.state().unwrap().plugins.is_empty());
    }

    #[test]
    fn a_tag_without_its_package_is_refused_clearly() {
        let mono = Mono::new();
        let m = mono.open();
        // tilr/store@2.0.0 points at a commit where only theme exists.
        fs::remove_dir_all(mono.repo.join("packages/store")).unwrap();
        mono.commit("drop store");
        mono.tag("store", "2.0.0");
        let error = mono.refuses("version_conflict", || {
            m.install_package(mono.source(), Some("tilr/store"), None, false)
        });
        assert!(
            error.to_string().contains("without package tilr/store"),
            "{error}"
        );
        let error = mono.refuses("invalid_argument", || {
            m.install_ref(mono.source(), Some("tilr/store@2.0.0"), None)
        });
        assert!(
            error.to_string().contains("not in this repository"),
            "{error}"
        );
        // Legacy v-tags are ambiguous in a multi-package repository.
        fixture_git(&mono.repo, &["tag", "v3.0.0"]);
        let error = mono.refuses("version_conflict", || {
            m.install_package(mono.source(), Some("tilr/theme"), None, false)
        });
        assert!(error.to_string().contains("ambiguous"), "{error}");
    }

    #[test]
    fn a_fork_of_a_package_releases_only_that_package() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        let destination = mono.dir.join("forks/me/theme");
        let result = m
            .fork_replace("tilr/theme", &destination, "me", None, false)
            .unwrap();
        assert!(result.failed.is_none(), "{:?}", result.error);
        assert_eq!(result.tag.as_deref(), Some("me/theme@1.0.0"));
        let config: packages::Config =
            serde_json::from_slice(&fs::read(destination.join(packages::CONFIG)).unwrap()).unwrap();
        assert_eq!(config.packages, ["packages/theme"]);
        let state = m.state().unwrap();
        assert_eq!(state.replacements["tilr/theme"], "me/theme");
        let fork = &state.plugins["me/theme"];
        assert_eq!(fork.path.as_deref(), Some("packages/theme"));
        assert_eq!(fork.fork_of.as_ref().unwrap().id, "tilr/theme");
        // A second fork_replace fails before anything is created.
        let error = m
            .fork_replace("tilr/theme", &destination, "me", None, false)
            .unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
    }

    #[test]
    fn a_fork_of_several_packages_starts_each_from_its_installed_revision() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        let old = fixture_git(&mono.repo, &["rev-parse", "HEAD"]);
        mono.write("theme", "1.1.0", &[("tilr/store", "^1.0")]);
        mono.commit("theme 1.1.0");
        mono.tag("theme", "1.1.0");
        mono.release("store", "1.2.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        let destination = mono.dir.join("forks/me/mono");
        let fork = m
            .fork_packages(&["tilr/theme", "tilr/store"], &destination, "me")
            .unwrap();
        assert!(fork.created);
        // theme 1.1.0 is the newest installed revision; store keeps its 1.0.0 files.
        assert_eq!(
            read(&destination.join("packages/store/dist/index.js")),
            "store 1.0.0"
        );
        assert_eq!(
            read(&destination.join("packages/theme/dist/index.js")),
            "theme 1.1.0"
        );
        let store = package_manifest(&destination, Some("packages/store")).unwrap();
        assert_eq!(store.version, "1.0.0");
        assert_eq!(store.fork_of.unwrap().commit, old);
        let theme = package_manifest(&destination, Some("packages/theme")).unwrap();
        assert_eq!(theme.dependencies["me/store"], "^1.0");
        assert_eq!(
            fork.rewrites,
            [DependencyRewrite {
                package: "me/theme".into(),
                from: "tilr/store".into(),
                to: "me/store".into(),
                requirement: "^1.0".into(),
                new_requirement: None,
            }]
        );
        assert!(fixture_git(&destination, &["status", "--porcelain"]).is_empty());

        // Refusals create nothing and leave the existing fork alone.
        let head = fixture_git(&destination, &["rev-parse", "HEAD"]);
        let other = mono.dir.join("forks/other");
        for (ids, namespace) in [
            (&["tilr/theme", "tilr/theme"][..], "you"),
            (&["tilr/theme", "tilr/missing"][..], "you"),
            (&["tilr/theme"][..], "tilr"),
            (&[][..], "you"),
        ] {
            assert!(m.fork_packages(ids, &other, namespace).is_err(), "{ids:?}");
        }
        assert!(!other.exists());
        let error = m
            .fork_packages(&["tilr/theme"], &destination, "me")
            .unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(error.kind(), "invalid_argument");
        // Adding needs the touched files of the target repository clean.
        let single = mono.dir.join("forks/single");
        m.fork_packages(&["tilr/store"], &single, "s").unwrap();
        let before = fixture_git(&single, &["rev-parse", "HEAD"]);
        fs::write(
            single.join(packages::CONFIG),
            "{\"packages\":[\"packages/*\"]}",
        )
        .unwrap();
        let error = m.fork_packages(&["tilr/theme"], &single, "s").unwrap_err();
        assert!(error.to_string().contains("commit or discard"), "{error}");
        assert!(!single.join("packages/theme").exists());
        assert_eq!(fixture_git(&single, &["rev-parse", "HEAD"]), before);
        assert_eq!(fixture_git(&destination, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn a_failed_release_of_several_packages_keeps_the_fork_and_installs_nothing() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        let result = m
            .fork_packages_replace(
                &["tilr/store", "tilr/theme"],
                &mono.dir.join("forks/me/mono"),
                "me",
                Some("2.0.0"),
                false,
            )
            .unwrap();
        assert!(result.error().is_none(), "{:?}", result.error());
        assert_eq!(
            result
                .packages
                .iter()
                .map(|p| p.tag.clone().unwrap())
                .collect::<Vec<_>>(),
            ["me/store@2.0.0", "me/theme@2.0.0"]
        );
        // A release whose tag exists fails for every package; the fork stays.
        let third = mono.dir.join("forks/third");
        let fork = m.fork_packages(&["tilr/store"], &third, "ns3").unwrap();
        fixture_git(&fork.path, &["tag", "ns3/theme@1.0.0"]);
        let result = m
            .fork_packages_replace(&["tilr/theme"], &third, "ns3", None, false)
            .unwrap();
        assert!(result.fork.packages.iter().all(|p| p.id == "ns3/theme"));
        let theme = &result.packages[0];
        assert_eq!(theme.failed, Some(ForkStep::Release));
        assert_eq!(result.error().unwrap().kind(), "version_conflict");
        assert!(third.join("packages/theme/plugin.json").is_file());
        assert!(!m.state().unwrap().plugins.contains_key("ns3/theme"));
    }

    // ---- regressions: forks, shared clones, layouts ------------------------------------

    fn head_of(repo: &Path) -> String {
        fixture_git(repo, &["rev-parse", "HEAD"])
    }
    impl Mono {
        fn clone_dir(&self) -> PathBuf {
            self.root.join("repos").join(&self.clones()[0])
        }
    }

    #[test]
    fn a_single_fork_of_a_shared_package_commits_on_the_fork_branch() {
        // B1: the shared clone's HEAD is theme's newer revision, store is installed older.
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        let store_commit = head_of(&mono.repo);
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        assert_ne!(head_of(&mono.clone_dir()), store_commit);
        let destination = mono.dir.join("forks/store");
        let path = m
            .fork_in_namespace("tilr/store", &destination, "me")
            .unwrap();
        assert_eq!(fixture_git(&path, &["branch", "--show-current"]), "fork");
        assert_eq!(
            fixture_git(&path, &["rev-parse", "refs/heads/fork"]),
            head_of(&path)
        );
        assert_eq!(fixture_git(&path, &["rev-parse", "HEAD^"]), store_commit);
        // Same tree semantics as several packages: the sibling is gone.
        assert!(!path.join("packages/theme").exists());
        let config: packages::Config =
            serde_json::from_slice(&fs::read(path.join(packages::CONFIG)).unwrap()).unwrap();
        assert_eq!(config.packages, ["packages/store"]);
        assert!(fixture_git(&path, &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn a_root_package_moved_into_a_folder_keeps_updating_and_forks() {
        // B2/B8: own clone (`plugins/<id>`) with a package folder.
        let dir = TempDir::new("moved");
        let repo = scaffold::init(&dir.join("x"), "acme", "x").unwrap().repo;
        let m = reopen(&dir.join("workspace")).unwrap();
        m.install(repo.to_str().unwrap()).unwrap();
        fs::create_dir_all(repo.join("packages/x")).unwrap();
        fs::rename(
            repo.join("plugin.json"),
            repo.join("packages/x/plugin.json"),
        )
        .unwrap();
        fs::rename(repo.join("dist"), repo.join("packages/x/dist")).unwrap();
        fs::write(
            repo.join(packages::CONFIG),
            r#"{"packages":["packages/*"]}"#,
        )
        .unwrap();
        scaffold::commit_package(&repo, "0.2.0", Some("acme/x")).unwrap();
        m.update("acme/x").unwrap();
        let state = m.state().unwrap();
        let p = &state.plugins["acme/x"];
        assert_eq!(p.path.as_deref(), Some("packages/x"));
        assert!(p.clone.is_none() && !p.is_root_package());
        fs::write(repo.join("packages/x/dist/index.js"), "x 0.3.0").unwrap();
        scaffold::commit_package(&repo, "0.3.0", Some("acme/x")).unwrap();
        let plan = m.update_all().unwrap();
        assert_eq!(plan.updates.len(), 1, "{plan:?}");
        assert_eq!(read(&dir.join("workspace/dist/acme/x/index.js")), "x 0.3.0");
        m.rollback("acme/x").unwrap();
        m.update("acme/x").unwrap();
        // Forked as a package folder, not refused as a root package.
        let fork = m
            .fork_packages(&["acme/x"], &dir.join("forks/x"), "me")
            .unwrap();
        assert_eq!(fork.packages[0].path, "packages/x");
        assert!(fork.path.join(packages::CONFIG).is_file());
        // forget/restore of the own clone finds the folder too.
        m.forget("acme/x").unwrap();
        let restored = m.restore("acme/x").unwrap();
        assert_eq!(restored.current.version, "0.3.0");
        assert_eq!(
            m.state().unwrap().plugins["acme/x"].path.as_deref(),
            Some("packages/x")
        );
    }

    #[test]
    fn a_forgotten_shared_package_keeps_its_clone_until_uninstalled() {
        // B4
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        m.forget("tilr/store").unwrap();
        let state = m.state().unwrap();
        assert!(!state.plugins.contains_key("tilr/store"));
        assert_eq!(
            state.forgotten["tilr/store"].path.as_deref(),
            Some("packages/store")
        );
        assert_eq!(mono.dist("store"), "store 1.0.0");
        let error = mono.refuses("already_installed", || {
            m.install_package(mono.source(), Some("tilr/store"), None, false)
        });
        assert!(error.to_string().contains("use restore"), "{error}");
        // The last registered user goes; the forgotten one still holds the clone.
        let plan = m.uninstall_plan("tilr/theme").unwrap();
        assert_eq!(plan.deletes.len(), 1, "{:?}", plan.deletes);
        m.uninstall("tilr/theme", false).unwrap();
        assert_eq!(mono.clones().len(), 1);
        let restored = m.restore("tilr/store").unwrap();
        assert_eq!(restored.current.version, "1.0.0");
        let state = m.state().unwrap();
        assert!(state.forgotten.is_empty());
        assert_eq!(
            state.plugins["tilr/store"].clone.as_deref(),
            Some(mono.clones()[0].as_str())
        );
        mono.release("store", "1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.1.0");
        // Uninstalling a forgotten package removes its files and the clone it was last on.
        m.forget("tilr/store").unwrap();
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert!(!plan.registered);
        assert_eq!(plan.deletes.len(), 2, "{:?}", plan.deletes);
        m.uninstall("tilr/store", false).unwrap();
        assert!(mono.clones().is_empty());
        assert!(!mono.root.join("dist/tilr/store").exists());
        let state = m.state().unwrap();
        assert!(state.plugins.is_empty() && state.forgotten.is_empty());
        mono.refuses("repository_not_found", || m.uninstall("tilr/store", true));
    }

    #[test]
    fn removing_a_package_moves_the_shared_head_to_a_remaining_one() {
        // B5: theme follows the dev channel, so HEAD is an untagged commit.
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        let store = m.state().unwrap().plugins["tilr/store"]
            .current
            .commit
            .clone();
        mono.write("theme", "1.1.0", &[]);
        let dev = mono.commit("theme work");
        m.update_channel("tilr/theme", None, true).unwrap();
        assert_eq!(head_of(&mono.clone_dir()), dev);
        // forget: HEAD goes to store's revision, store keeps updating.
        m.forget("tilr/theme").unwrap();
        assert_eq!(head_of(&mono.clone_dir()), store);
        m.restore("tilr/theme").unwrap();
        assert_eq!(head_of(&mono.clone_dir()), dev);
        // uninstall with local work in the clone: HEAD moves, the file stays.
        fs::write(mono.clone_dir().join("scratch.txt"), "mine").unwrap();
        m.uninstall("tilr/theme", false).unwrap();
        assert_eq!(head_of(&mono.clone_dir()), store);
        assert_eq!(read(&mono.clone_dir().join("scratch.txt")), "mine");
        fs::remove_file(mono.clone_dir().join("scratch.txt")).unwrap();
        mono.release("store", "1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.1.0");

        // An edit that the move would overwrite refuses the removal; nothing changes.
        mono.write("theme", "1.2.0", &[]);
        mono.commit("more theme work");
        m.install_package(mono.source(), Some("tilr/theme"), None, true)
            .unwrap();
        fs::write(
            mono.clone_dir().join("packages/theme/dist/index.js"),
            "edit",
        )
        .unwrap();
        let error = mono.refuses("local_changes", || m.uninstall("tilr/theme", true));
        assert!(
            error
                .to_string()
                .contains("must move to the installed revision"),
            "{error}"
        );
        assert_eq!(
            read(&mono.clone_dir().join("packages/theme/dist/index.js")),
            "edit"
        );
    }

    #[test]
    fn a_defective_sibling_does_not_block_a_package() {
        // B6: theme has no dist/ at store's 1.1.0 tag.
        let mono = Mono::new();
        let m = mono.open();
        fs::remove_dir_all(mono.repo.join("packages/theme/dist")).unwrap();
        mono.release("store", "1.1.0");
        let result = m
            .install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        assert_eq!(result.current.version, "1.1.0");
        assert_eq!(mono.dist("store"), "store 1.1.0");
        let head = head_of(&mono.repo);
        let error = packages::find_at(&mono.repo, &head, Some("tilr/theme")).unwrap_err();
        assert!(error.to_string().contains("ready dist/"), "{error}");
        // Strict listing still reports the defect; the lenient one keeps store.
        assert!(packages::packages_at(&mono.repo, &head).is_err());
        let layout = packages::layout_at(&mono.repo, &head).unwrap().unwrap();
        assert_eq!(layout.packages.len(), 1);
        assert_eq!(layout.problems[0].id.as_deref(), Some("tilr/theme"));
        // theme's own releases: the old tag stays valid, the new commit is not.
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
    }

    #[test]
    fn a_package_can_move_to_another_folder_between_releases() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        fs::create_dir_all(mono.repo.join("apps")).unwrap();
        fs::rename(
            mono.repo.join("packages/store"),
            mono.repo.join("apps/store"),
        )
        .unwrap();
        fs::write(
            mono.repo.join(packages::CONFIG),
            r#"{"packages":["packages/*","apps/*"]}"#,
        )
        .unwrap();
        let mut moved = make_manifest("tilr/store", "1.1.0");
        moved.dependencies.clear();
        write_manifest(&mono.repo.join("apps/store"), &moved);
        fs::write(mono.repo.join("apps/store/dist/index.js"), "store moved").unwrap();
        mono.commit("move store");
        mono.tag("store", "1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store moved");
        assert_eq!(
            m.state().unwrap().plugins["tilr/store"].path.as_deref(),
            Some("apps/store")
        );
        m.rollback("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.0.0");
        assert_eq!(
            m.state().unwrap().plugins["tilr/store"].path.as_deref(),
            Some("packages/store")
        );
        m.update_all().unwrap();
        assert_eq!(mono.dist("store"), "store moved");
    }

    #[test]
    fn a_fork_never_drops_edits_of_a_package_installed_at_an_older_revision() {
        // B10: store is installed at the first commit, its folder changed since.
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        mono.release("store", "1.1.0");
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        let clone = mono.clone_dir();
        fs::write(clone.join("packages/store/dist/index.js"), "store edit").unwrap();
        let destination = mono.dir.join("forks/both");
        let error = mono.refuses("local_changes", || {
            m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
        });
        assert!(error.to_string().contains("older revision"), "{error}");
        assert!(!destination.exists());
        fixture_git(&clone, &["checkout", "-q", "--", "packages/store"]);
        m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
            .unwrap();
        assert_eq!(
            read(&destination.join("packages/store/dist/index.js")),
            "store 1.0.0"
        );
        // A missing installed revision is reported as such, not as diverged history.
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(mono.root.join("state.json")).unwrap()).unwrap();
        state["plugins"]["tilr/store"]["current"]["commit"] = "1".repeat(40).into();
        fs::write(mono.root.join("state.json"), state.to_string()).unwrap();
        let error = m
            .fork_packages(
                &["tilr/store"],
                &mono.dir.join("new/parents/missing"),
                "you",
            )
            .unwrap_err();
        assert_eq!(error.kind(), "git", "{error}");
        assert!(!error.to_string().contains("diverged"), "{error}");
        assert!(
            !mono.dir.join("new").exists(),
            "created parents are removed"
        );
    }

    #[test]
    fn adding_to_a_fork_leaves_no_temporary_folder_in_the_repository() {
        // B12: a symlink in theme's folder makes its export fail after the checks.
        let mono = Mono::new();
        mono.write("theme", "1.0.1", &[]);
        std::os::unix::fs::symlink("plugin.json", mono.repo.join("packages/theme/link")).unwrap();
        mono.commit("link");
        mono.tag("theme", "1.0.1");
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        let destination = mono.dir.join("forks/me");
        m.fork_packages(&["tilr/store"], &destination, "me")
            .unwrap();
        let head = head_of(&destination);
        let before = tree_hash(&destination);
        let error = m
            .fork_packages(&["tilr/theme"], &destination, "me")
            .unwrap_err();
        assert_eq!(error.kind(), "unsupported", "{error}");
        assert_eq!(head_of(&destination), head);
        assert_eq!(
            tree_hash(&destination),
            before,
            "repository changed by a failed add"
        );
        assert!(fs::read_dir(&destination).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".gitpm")
        }),);
    }

    #[test]
    fn a_dependency_cycle_between_forks_is_refused_before_anything_is_created() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        mono.write("theme", "1.1.0", &[("tilr/store", "^1.0")]);
        mono.commit("theme needs store");
        mono.tag("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        mono.write("store", "1.1.0", &[("tilr/theme", "^1.0")]);
        mono.commit("store needs theme");
        mono.tag("store", "1.1.0");
        m.update("tilr/store").unwrap();
        let destination = mono.dir.join("forks/cycle");
        let error = mono.refuses("dependency_conflict", || {
            m.fork_packages_replace(
                &["tilr/store", "tilr/theme"],
                &destination,
                "me",
                None,
                false,
            )
        });
        assert!(error.to_string().contains("dependency cycle"), "{error}");
        assert!(
            error
                .to_string()
                .contains("tilr/store → tilr/theme → tilr/store"),
            "{error}"
        );
        assert!(!destination.exists());
        // Without the install step a cyclic pair can still be forked.
        m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
            .unwrap();
    }

    #[test]
    fn a_workspace_of_gitpm_0_3_opens_and_is_written_as_format_2() {
        let (ws, m) = Ws::installed("dan/demo1");
        drop(m);
        let path = ws.root.join("state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        state.as_object_mut().unwrap().remove("format");
        fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
        let m = ws.open();
        assert_eq!(m.state().unwrap().format, 1);
        assert_eq!(m.list().unwrap().len(), 1);
        m.disable("dan/demo1").unwrap();
        assert_eq!(m.state().unwrap().format, 2);
        assert!(read(&path).starts_with("{\n  \"format\": 2,"));
        drop(m);
        state["format"] = 3.into();
        fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
        let error = reopen(&ws.root).err().unwrap();
        assert_eq!(error.kind(), "unsupported");
        assert!(error.to_string().contains("newer gitpm"), "{error}");
    }

    // ---- revisions, tags and records of shared clones ----------------------------------

    fn keep_refs(repo: &Path) -> Vec<String> {
        fixture_git(
            repo,
            &["for-each-ref", "--format=%(refname)", "refs/gitpm/"],
        )
        .lines()
        .map(str::to_owned)
        .collect()
    }
    fn gc(repo: &Path) {
        fixture_git(repo, &["reflog", "expire", "--expire=now", "--all"]);
        fixture_git(repo, &["gc", "-q", "--prune=now"]);
    }
    /// Everything of a repository except its object store.
    fn snapshot(repo: &Path) -> String {
        crate::test_support::tree_hash_except(repo, &[".git/objects"])
    }
    fn failing_hook(repo: &Path) {
        let hook = repo.join(".git/hooks/pre-commit");
        fs::create_dir_all(hook.parent().unwrap()).unwrap();
        fs::write(&hook, "#!/bin/sh\necho refused by hook >&2\nexit 1\n").unwrap();
        fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    }

    #[test]
    fn installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc() {
        let mono = Mono::new();
        let first = fixture_git(&mono.repo, &["rev-parse", "HEAD"]);
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        // theme follows the dev channel to an untagged commit; 1.1.0 is its previous.
        mono.write("theme", "1.2.0", &[]);
        let dev = mono.commit("theme dev");
        m.update_channel("tilr/theme", None, true).unwrap();
        assert_eq!(m.state().unwrap().plugins["tilr/theme"].current.commit, dev);
        let clone = mono.clone_dir();
        assert_eq!(
            keep_refs(&clone),
            [
                "refs/gitpm/tilr/store/current",
                "refs/gitpm/tilr/theme/current",
                "refs/gitpm/tilr/theme/previous"
            ]
        );
        // Upstream drops theme's history and its tag; store releases on the rewrite.
        fixture_git(&mono.repo, &["reset", "-q", "--hard", &first]);
        fixture_git(&mono.repo, &["tag", "-d", "tilr/theme@1.1.0"]);
        mono.release("store", "1.1.0");
        // Updating store prunes and moves HEAD: only the keep refs hold theme's revisions.
        m.update("tilr/store").unwrap();
        gc(&clone);
        for commit in [
            &dev,
            &m.state().unwrap().plugins["tilr/theme"]
                .previous
                .clone()
                .unwrap()
                .commit,
        ] {
            fixture_git(&clone, &["cat-file", "-e", &format!("{commit}^{{commit}}")]);
        }
        let plan = m.update_all().unwrap();
        assert!(plan.updates.is_empty(), "{plan:?}");
        m.rollback("tilr/theme").unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.1.0");
        m.rollback("tilr/theme").unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.2.0");
        let fork = m
            .fork_packages(&["tilr/theme"], &mono.dir.join("theme-fork"), "me")
            .unwrap();
        assert_eq!(
            read(&fork.path.join("packages/theme/dist/index.js")),
            "theme 1.2.0"
        );
        assert!(
            keep_refs(&fork.path).is_empty(),
            "the fork has no workspace refs"
        );
        // Records that go take their refs with them.
        m.forget("tilr/store").unwrap();
        assert!(keep_refs(&clone).contains(&"refs/gitpm/tilr/store/current".to_owned()));
        m.uninstall("tilr/store", false).unwrap();
        assert_eq!(
            keep_refs(&clone),
            [
                "refs/gitpm/tilr/theme/current",
                "refs/gitpm/tilr/theme/previous"
            ]
        );
        // Upstream dropped theme's revisions: they are local work now.
        m.uninstall("tilr/theme", true).unwrap();
        assert!(mono.clones().is_empty());
    }

    #[test]
    fn a_moved_tag_of_a_package_that_is_not_installed_is_taken_from_upstream() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), None, false)
            .unwrap();
        // theme 1.0.0 is retagged upstream (it was never installed here).
        fs::write(
            mono.repo.join("packages/theme/dist/index.js"),
            "theme retagged",
        )
        .unwrap();
        let moved = mono.commit("retag theme");
        fixture_git(&mono.repo, &["tag", "-f", "tilr/theme@1.0.0"]);
        let installed = m
            .install_package(mono.source(), Some("tilr/theme"), None, false)
            .unwrap();
        assert_eq!(installed.current.commit, moved);
        assert_eq!(mono.dist("theme"), "theme retagged");
        assert_eq!(mono.dist("store"), "store 1.0.0");
    }

    #[test]
    fn a_moved_tag_of_an_installed_package_refuses_install_and_update_of_a_sibling() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        mono.write("dock", "1.0.0", &[]);
        mono.commit("dock");
        mono.tag("dock", "1.0.0");
        mono.release("theme", "1.1.0");
        fs::write(
            mono.repo.join("packages/store/dist/index.js"),
            "store moved",
        )
        .unwrap();
        mono.commit("move store");
        fixture_git(&mono.repo, &["tag", "-f", "tilr/store@1.0.0"]);
        for error in [
            mono.refuses("version_conflict", || {
                m.install_package(mono.source(), Some("tilr/dock"), None, false)
            }),
            mono.refuses("version_conflict", || m.update("tilr/theme")),
        ] {
            assert!(
                error.to_string().contains(
                    "release tag tilr/store@1.0.0 of the installed tilr/store changed commit"
                ),
                "{error}"
            );
        }
    }

    #[test]
    fn a_forgotten_record_whose_clone_is_gone_can_be_uninstalled() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        m.forget("tilr/store").unwrap();
        m.uninstall("tilr/theme", false).unwrap();
        fs::remove_dir_all(mono.clone_dir()).unwrap();
        let error = mono.refuses("workspace", || m.restore("tilr/store"));
        assert!(
            error.to_string().contains("is gone; uninstall tilr/store"),
            "{error}"
        );
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert!(!plan.registered && plan.lost_work.is_empty());
        assert_eq!(plan.deletes.len(), 1, "{:?}", plan.deletes);
        m.uninstall("tilr/store", false).unwrap();
        let state = m.state().unwrap();
        assert!(state.forgotten.is_empty() && state.plugins.is_empty());
        assert!(!mono.root.join("dist/tilr/store").exists());
    }

    #[test]
    fn adding_to_a_fork_refuses_a_failing_commit_and_a_missing_identity_untouched() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), None, false)
                .unwrap();
        }
        let destination = mono.dir.join("forks/me");
        // The new fork repository is gitpm's own: the user's hooks do not run there.
        m.fork_packages(&["tilr/store"], &destination, "me")
            .unwrap();
        // In the author's repository a pre-commit hook that fails refuses the add.
        failing_hook(&destination);
        let before = snapshot(&destination);
        let error = m
            .fork_packages(&["tilr/theme"], &destination, "me")
            .unwrap_err();
        assert_eq!(error.kind(), "git", "{error}");
        assert!(
            error.to_string().contains("the repository was restored")
                && error.to_string().contains("refused by hook"),
            "{error}"
        );
        assert_eq!(snapshot(&destination), before, "repository changed");
        fs::remove_file(destination.join(".git/hooks/pre-commit")).unwrap();
        // Without a Git identity gitpm refuses instead of writing one.
        fixture_git(&destination, &["config", "user.name", ""]);
        let before = snapshot(&destination);
        let error = m
            .fork_packages(&["tilr/theme"], &destination, "me")
            .unwrap_err();
        assert_eq!(error.kind(), "git", "{error}");
        assert!(error.to_string().contains("GIT_AUTHOR_IDENT"), "{error}");
        assert_eq!(snapshot(&destination), before, "repository changed");
        assert_eq!(
            fixture_git(&destination, &["config", "--local", "user.name"]),
            ""
        );
    }
}
