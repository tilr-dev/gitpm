//! Workspace: locked directory with plugin clones, deployed `dist/` and `state.json`.
//! Every modification runs on a full staged copy and is switched in with two renames.
use crate::error::{Context, Error, Require, Result, ensure};
use crate::{
    ForkOf, Installed, InstalledPlugin, LostWork, Manifest, Revision, State, catalog,
    check::{self, CheckRow},
    exec::git,
    identity,
    lost_work::{self, clean},
    model::{manifest, manifest_json, resolve_ref, resolve_upstream_ref, revision},
    releases, scaffold, valid_id,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
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
}

pub struct Manager {
    root: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    options: Options,
    /// Serializes operations of this handle (stage/backup paths are shared).
    busy: Mutex<()>,
    _lock: File,
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let meta =
        fs::symlink_metadata(from).context(Error::Io, format!("missing {}", from.display()))?;
    ensure!(
        !meta.file_type().is_symlink(),
        Unsupported,
        "symlinks are not supported: {}",
        from.display()
    );
    if meta.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
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
fn remove(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
fn deploy(root: &Path, id: &str) -> Result<()> {
    let dest = root.join("dist").join(id);
    remove(&dest)?;
    copy_tree(&root.join("plugins").join(id).join("dist"), &dest)
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
        let name = root
            .file_name()
            .require(Error::Workspace, "workspace needs a directory name")?
            .to_str()
            .require(Error::Workspace, "non-UTF8 workspace name")?;
        let parent = root
            .parent()
            .require(Error::Workspace, "workspace needs a parent")?;
        fs::create_dir_all(parent)?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(parent.join(format!(".{name}.lock")))?;
        lock.try_lock()
            .map_err(|_| Error::Locked("workspace is already in use".into()))?;
        let manager = Self {
            root: root.clone(),
            stage: parent.join(format!(".{name}.stage")),
            backup: parent.join(format!(".{name}.backup")),
            options,
            busy: Mutex::new(()),
            _lock: lock,
        };
        if manager.backup.exists() {
            if !root.exists() {
                fs::rename(&manager.backup, &root)?;
            } else {
                remove(&manager.backup)?;
            }
        }
        remove(&manager.stage)?;
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
    fn transaction<T>(&self, action: impl FnOnce(&Path, &mut State) -> Result<T>) -> Result<T> {
        let _busy = self.busy();
        let mut state = self.state()?;
        copy_tree(&self.root, &self.stage)?;
        let result = (|| {
            let value = action(&self.stage, &mut state)?;
            fs::write(
                self.stage.join("state.json"),
                serde_json::to_vec_pretty(&state)?,
            )?;
            fs::rename(&self.root, &self.backup)?;
            if let Err(error) = fs::rename(&self.stage, &self.root) {
                fs::rename(&self.backup, &self.root)?;
                return Err(error.into());
            }
            remove(&self.backup)?;
            Ok(value)
        })();
        if self.stage.exists() {
            remove(&self.stage)?;
        }
        result
    }

    /// Compare installed versions with the configured catalog. Read-only.
    pub fn check(&self) -> Result<Vec<CheckRow>> {
        let state = self.state()?;
        if state.plugins.is_empty() {
            return Ok(Vec::new());
        }
        let snapshot = catalog::selected(self.catalog_location()?, &state)?;
        Ok(check::compare(&state, &snapshot))
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
            copy_tree(&source, &destination)?;
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
        self.install_checked(source, reference, expected, None)
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
    pub fn install_release(&self, release: &catalog::Release) -> Result<InstallResult> {
        catalog::validate_release(release, &release.manifest.id, &release.manifest.version)?;
        self.install_checked(
            &release.source,
            Some(&release.commit),
            Some(&release.manifest),
            if release.files.is_empty() {
                None
            } else {
                Some(&release.files)
            },
        )
    }
    /// Look up `id` (latest if `version` is `None`) in the configured catalog and install it.
    pub fn install_from_catalog(&self, id: &str, version: Option<&str>) -> Result<InstallResult> {
        let release = self.catalog_release(id, version)?;
        self.install_release(&release)
    }
    fn install_checked(
        &self,
        source: &str,
        reference: Option<&str>,
        expected: Option<&Manifest>,
        files: Option<&BTreeMap<String, catalog::FileDigest>>,
    ) -> Result<InstallResult> {
        ensure!(
            !source.starts_with('-'),
            InvalidArgument,
            "invalid git source"
        );
        let source = if Path::new(source).exists() {
            fs::canonicalize(source)?.to_string_lossy().into_owned()
        } else {
            source.to_owned()
        };
        self.transaction(|root, state| {
            let repo = root.join("incoming");
            ensure!(!repo.exists(), Workspace, "unexpected incoming directory");
            git(
                root,
                &["clone", "--no-hardlinks", "--", &source, "incoming"],
            )?;
            let commit = match reference {
                Some(reference) => resolve_ref(&repo, reference)?,
                None => releases::latest(&repo, None)?.commit,
            };
            git(&repo, &["checkout", "--detach", &commit])?;
            let m = manifest(&repo)?;
            if let Some(expected) = expected {
                ensure!(
                    serde_json::to_value(&m)? == serde_json::to_value(expected)?,
                    Catalog,
                    "catalog manifest does not match git revision"
                );
            }
            if let Some(expected_files) = files {
                ensure!(
                    catalog::hash_files(&repo.join("dist"))? == *expected_files,
                    Catalog,
                    "catalog file hashes do not match git dist"
                );
            }
            if state.plugins.contains_key(&m.id) {
                return Err(Error::AlreadyInstalled {
                    message: format!("already installed: {}", m.id),
                    id: m.id,
                });
            }
            // A legacy ID can be a prefix of a scoped ID; never nest a repository in another.
            ensure!(
                !state
                    .plugins
                    .keys()
                    .any(|id| id.starts_with(&format!("{}/", m.id))
                        || m.id.starts_with(&format!("{id}/"))),
                Workspace,
                "plugin ID conflicts with an installed namespace path"
            );
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
                    id: m.id,
                });
            }
            let current = revision(&repo, &m)?;
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::rename(repo, dest)?;
            deploy(root, &m.id)?;
            let installed = Installed {
                source: source.clone(),
                current: current.clone(),
                previous: None,
                enabled: true,
                fork_of: m.fork_of.clone(),
            };
            state.plugins.insert(m.id.clone(), installed);
            Ok(InstallResult {
                id: m.id,
                source: source.clone(),
                current,
                fork_of: m.fork_of,
            })
        })
    }
    pub fn update(&self, id: &str) -> Result<RevisionChange> {
        self.update_ref(id, None)
    }
    pub fn update_ref(&self, id: &str, reference: Option<&str>) -> Result<RevisionChange> {
        self.update_channel(id, reference, false)
    }
    /// Update to `reference`, the latest release tag (stable) or origin HEAD (dev).
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
        self.transaction(|root, state| {
            let p = state
                .plugins
                .get_mut(id)
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
            let repo = root.join("plugins").join(id);
            clean(&repo, id)?;
            lost_work::require_head(
                &repo,
                id,
                &p.current.commit,
                "local commits detected; update would discard them",
            )?;
            git(&repo, &["fetch", "origin", "--tags"])?;
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
            let next = revision(&repo, &m)?;
            let from = p.current.clone();
            p.previous = Some(p.current.clone());
            p.current = next.clone();
            p.fork_of = m.fork_of.clone();
            deploy(root, id)?;
            Ok(RevisionChange {
                id: id.into(),
                from,
                to: next,
                changed: true,
            })
        })
    }
    /// Swap current and previous version (offline).
    pub fn rollback(&self, id: &str) -> Result<RevisionChange> {
        valid_id(id)?;
        self.transaction(|root, state| {
            let p = state
                .plugins
                .get_mut(id)
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
            let previous = p
                .previous
                .clone()
                .ok_or_else(|| Error::NoPreviousVersion { id: id.into() })?;
            let repo = root.join("plugins").join(id);
            clean(&repo, id)?;
            lost_work::require_head(&repo, id, &p.current.commit, "local commits detected")?;
            git(&repo, &["checkout", "--detach", &previous.commit])?;
            let m = manifest(&repo)?;
            ensure!(
                m.id == id && m.version == previous.version,
                InvalidManifest,
                "rollback manifest mismatch"
            );
            let from = p.current.clone();
            p.previous = Some(p.current.clone());
            p.current = previous.clone();
            p.fork_of = m.fork_of.clone();
            deploy(root, id)?;
            Ok(RevisionChange {
                id: id.into(),
                from,
                to: previous,
                changed: true,
            })
        })
    }
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|_, state| {
            state
                .plugins
                .get_mut(id)
                .ok_or_else(|| Error::NotInstalled { id: id.into() })?
                .enabled = enabled;
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
        valid_id(id)?;
        self.transaction(|_, state| {
            if state.plugins.remove(id).is_none() {
                return Err(Error::NotInstalled { id: id.into() });
            }
            Ok(())
        })
    }
    /// Re-register a retained (forgotten) clean repository; previous history is not restored.
    pub fn restore(&self, id: &str) -> Result<InstallResult> {
        valid_id(id)?;
        self.transaction(|root, state| {
            if state.plugins.contains_key(id) {
                return Err(Error::AlreadyInstalled {
                    id: id.into(),
                    message: "plugin is already registered".into(),
                });
            }
            let repo = plugin_repo(root, id)?;
            clean(&repo, id)?;
            let m = manifest(&repo)?;
            ensure!(m.id == id, InvalidManifest, "plugin id mismatch");
            let installed = Installed {
                source: git(&repo, &["remote", "get-url", "origin"])?,
                current: revision(&repo, &m)?,
                previous: None,
                enabled: true,
                fork_of: m.fork_of.clone(),
            };
            let result = InstallResult {
                id: id.into(),
                source: installed.source.clone(),
                current: installed.current.clone(),
                fork_of: installed.fork_of.clone(),
            };
            state.plugins.insert(id.to_owned(), installed);
            deploy(root, id)?;
            Ok(result)
        })
    }
    /// Read-only: what `uninstall` would delete and which local work would be lost.
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
        })
    }
    /// Remove state entry, repository and dist. With `confirm == false` any local work
    /// (changes, untracked/ignored files, unpublished commits/branches/tags, stash)
    /// refuses with [`Error::LocalChanges`]; `confirm == true` deletes it anyway.
    pub fn uninstall(&self, id: &str, confirm: bool) -> Result<()> {
        valid_id(id)?;
        self.transaction(|root, state| {
            let repo = plugin_repo(root, id)?;
            if !confirm {
                let recorded = state.plugins.get(id).map(|p| p.current.commit.as_str());
                let items = lost_work::detect(&repo, recorded)?;
                if !items.is_empty() {
                    return Err(lost_work::refusal(id, items));
                }
            }
            remove(&repo)?;
            remove(&root.join("dist").join(id))?;
            state.plugins.remove(id);
            Ok(())
        })
    }
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
            for name in [".workspace.stage", ".workspace.backup"] {
                assert!(!self.dir.join(name).exists(), "{name} left behind");
            }
            assert!(!self.root.join("incoming").exists(), "incoming left behind");
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
        copy_tree(&from, &dir.join("to")).unwrap();
        assert_eq!(tree_hash(&dir.join("to")), tree_hash(&from));

        std::os::unix::fs::symlink("top", from.join("a/link")).unwrap();
        let error = copy_tree(&from, &dir.join("to2")).unwrap_err();
        assert_eq!(error.kind(), "unsupported");
        assert!(error.to_string().contains("symlinks are not supported"));
        let error = copy_tree(&dir.join("missing"), &dir.join("to3")).unwrap_err();
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

    #[test]
    fn open_recovers_an_interrupted_switch() {
        let (ws, m) = Ws::installed("dan/demo1");
        drop(m);
        let state = read(&ws.root.join("state.json"));
        let backup = ws.dir.join(".workspace.backup");
        let stage = ws.dir.join(".workspace.stage");
        // Crash between the two renames: only the backup exists.
        fs::rename(&ws.root, &backup).unwrap();
        fs::create_dir_all(stage.join("half")).unwrap();
        let m = ws.open();
        assert_eq!(read(&ws.root.join("state.json")), state);
        assert_eq!(m.list().unwrap()[0].id, "dan/demo1");
        ws.no_leftovers();
        drop(m);
        // Crash after the switch: the old backup is stale and removed.
        copy_tree(&ws.root, &backup).unwrap();
        fs::write(backup.join("state.json"), "{stale").unwrap();
        let m = ws.open();
        assert_eq!(read(&ws.root.join("state.json")), state);
        ws.no_leftovers();
        drop(m);
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
            assert_eq!(error.to_string(), "plugin is not installed");
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
            .insert("dan/demo1".into(), "^0.1.0".into());
        write_manifest(&deps.path, &with_deps);
        fixture_git(&deps.path, &["commit", "-q", "-am", "deps"]);
        ws.refuses("unsupported", || {
            m.install_ref(deps.source(), Some("HEAD"), None)
        });

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
        assert_eq!(error.to_string(), "plugin is already registered");
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
        copy_tree(&ws.repo(), &ws.root.join("plugins/dan/alias2")).unwrap();
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
    #[ignore = "known defect: a symlink outside dist/ in one plugin blocks every workspace operation"]
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
    }
}
