//! Workspace: locked directory with plugin clones, deployed `dist/` and `state.json`.
//! Every modification runs on a full staged copy and is switched in with two renames.
use crate::error::{Context, Error, Require, Result, ensure};
use crate::{
    ForkOf, Installed, InstalledPlugin, LostWork, Manifest, Revision, State, catalog,
    check::{self, CheckRow},
    exec::git,
    identity,
    lost_work::{self, clean},
    model::{manifest, manifest_json, resolve_ref, revision},
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
                &["remote", "add", "upstream", &installed.source],
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
            if dev {
                git(&repo, &["remote", "set-head", "origin", "--auto"])?;
            }
            let target = if let Some(reference) = reference {
                resolve_ref(&repo, reference)?
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
            ensure!(
                m.version != p.current.version,
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
            let repo = root.join("plugins").join(id);
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
    fn assert_send_sync<T: Send + Sync>() {}
    #[test]
    fn manager_is_send_and_sync() {
        assert_send_sync::<Manager>();
    }
}
