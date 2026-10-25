//! Workspace: locked directory with package clones, deployed `dist/` and `state.json`.
//! Every modification stages only the paths it touches and commits them through a
//! journal (see `txn`), so a crash at any point leaves a consistent workspace.
//!
//! `dist/<id>` exists exactly for the active packages ([`State::is_active`]: installed,
//! enabled, not replaced). Every mutating operation brings all of `dist/` in line with
//! that rule in its transaction (see `settle_dist`); [`Manager::sync_dist`] does it on
//! its own (an explicit repair, for example after a folder was copied into `dist/` by hand).
use crate::error::{Context, Error, Require, Result, ensure, fail};
use crate::{
    ForkOf, Installed, InstalledPackage, LostWork, Manifest, Mode, Replaces, Revision, State,
    catalog,
    check::{self, CheckRow},
    deps, identity,
    lost_work::{self, clean},
    model::{manifest_json, package_manifest, resolve_ref, resolve_upstream_ref, revision},
    packages, releases, scaffold, transport,
    txn::{self, Txn},
    valid_id,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

mod builds;
pub(crate) mod clones;
mod fork;
mod link;
use clones::{guard_clone, installed_repo, place_clone, removal, source_repo};
pub use fork::{DependencyRewrite, ForkedPackage, PackageFork};
pub use link::LinkResult;

/// A mode in messages: `release`, `pinned at REF`, `branch NAME`.
fn mode_text(mode: &Mode) -> String {
    match mode {
        Mode::Release => "release".into(),
        Mode::Pinned(reference) => format!("pinned at {reference}"),
        Mode::Branch(branch) => format!("branch {branch}"),
        Mode::Link(_) => "dev-linked".into(),
    }
}

/// Whether a fetched package is recorded enabled (install, optionally forced past
/// inactive dependencies) or disabled (download).
#[derive(Clone, Copy)]
enum Activation {
    Download,
    Install { force: bool },
}

/// Settings passed explicitly by the caller; the library never reads environment variables.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Catalog location (local folder, JSON file or HTTP(S) URL) for catalog-based operations.
    pub catalog: Option<String>,
    /// Preserve the SSH transport supplied by the caller's environment.
    pub custom_ssh: bool,
}

/// What an install installs and the mode it records (see [`Mode`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InstallTarget {
    /// The newest release; release mode.
    #[default]
    Release,
    /// A tag or commit (pinned) or a branch of the origin (followed); `HEAD` is the
    /// origin's default branch.
    Ref(String),
    /// The origin's default branch, followed; its name is recorded.
    Dev,
}
/// Where an update moves a package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Follow the recorded mode: release mode moves to the newest release, branch mode to
    /// the branch HEAD, a pinned package stays (reason [`UnchangedReason::Pinned`]).
    #[default]
    Recorded,
    /// Back to release mode at the newest release.
    Release,
    /// Move to a ref and record its mode as an install does.
    Ref(String),
    /// Follow the origin's default branch.
    Dev,
}
/// Options of [`Manager::update_with`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    /// Drop the protection of a package installed from a verified catalog (its record
    /// loses `catalog`) and update it from its origin like a direct install, in the same
    /// transaction. Refused with `invalid_argument` for a package that is not protected.
    pub detach: bool,
}
impl From<InstallTarget> for Target {
    fn from(target: InstallTarget) -> Self {
        match target {
            InstallTarget::Release => Target::Release,
            InstallTarget::Ref(reference) => Target::Ref(reference),
            InstallTarget::Dev => Target::Dev,
        }
    }
}
/// Why an update left a package where it is although it was asked to move it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnchangedReason {
    /// Live link, unchanged; discovery is separate from this disposition.
    DevLinked {
        availability: Box<CheckRow>,
        dependency_problems: Vec<String>,
    },
    /// Pinned to `reference` (a tag or commit): only an explicit target moves it.
    Pinned { reference: String },
    /// Branch mode: the history of `branch` on the origin no longer contains the
    /// installed commit (`update all` only; `update ID` refuses with `version_conflict`).
    Diverged { branch: String },
    /// Branch mode: the origin no longer has `branch` (`update all` only; `update ID`
    /// refuses with `version_conflict`).
    BranchGone { branch: String },
    /// A broken consumer: its direct dependency `dependency` is not satisfied now
    /// (`problem` as `list` reports it, for example `not installed` after a forced
    /// removal). `update all` leaves it out of its solve and never installs the
    /// dependency for it (`update all` only; `update ID` refuses with
    /// `dependency_conflict`).
    BrokenDependency { dependency: String, problem: String },
    /// A package protected by a verified catalog: the origin has the newer release
    /// `version`, which the catalog does not list, so it is not installed.
    NotInCatalog { version: String },
    /// A package protected by a verified catalog whose catalog could not be used (not
    /// reachable, invalid, or no longer verified; `reason` is the error message). `update
    /// all` only; `update ID` fails with `catalog` (or `http`).
    CatalogUnavailable { reason: String },
}
impl fmt::Display for UnchangedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnchangedReason::DevLinked {
                availability,
                dependency_problems,
            } => {
                let result = if availability.update_available {
                    write!(
                        f,
                        "version {} is available; package is dev-linked and was not updated",
                        availability.latest.as_deref().unwrap_or("unknown")
                    )
                } else if let Some(problem) = &availability.problem {
                    write!(f, "dev-linked and unchanged; release discovery: {problem}")
                } else {
                    write!(f, "dev-linked and unchanged; {}", availability.message())
                };
                result?;
                if !dependency_problems.is_empty() {
                    write!(f, "; dependencies: {}", dependency_problems.join("; "))?;
                }
                Ok(())
            }
            UnchangedReason::Pinned { reference } => write!(f, "pinned to {reference}"),
            UnchangedReason::Diverged { branch } => write!(f, "history of {branch} diverged"),
            UnchangedReason::BranchGone { branch } => write!(f, "branch {branch} is gone"),
            UnchangedReason::BrokenDependency {
                dependency,
                problem,
            } if problem == deps::NOT_INSTALLED => {
                write!(f, "dependency {dependency} is not installed")
            }
            UnchangedReason::BrokenDependency {
                dependency,
                problem,
            } => write!(f, "dependency {dependency}: {problem}"),
            UnchangedReason::NotInCatalog { version } => {
                write!(f, "upstream {version} not in catalog")
            }
            UnchangedReason::CatalogUnavailable { reason } => {
                write!(f, "catalog unavailable: {reason}")
            }
        }
    }
}
/// Why `update all` selected an older release than the newest one of a package (see
/// [`HeldBack`]). [`fmt::Display`] gives the message the CLI prints.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeldBackReason {
    /// The requirements of other packages (or of its own newer releases) do not allow
    /// the newer release.
    Constraints,
    /// The newest release needs `dependency` `requirement`, which is not installed, and
    /// no catalog is configured to install it from.
    MissingDependency {
        dependency: String,
        requirement: String,
    },
    /// The package replaces `original` and its newest release no longer declares
    /// `replaces` for it.
    NoLongerReplaces { original: String },
}
impl fmt::Display for HeldBackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeldBackReason::Constraints => f.write_str("dependency constraints"),
            HeldBackReason::MissingDependency {
                dependency,
                requirement,
            } => write!(
                f,
                "needs {dependency} {requirement}, which is not installed and no catalog is configured"
            ),
            HeldBackReason::NoLongerReplaces { original } => {
                write!(f, "no longer replaces {original}")
            }
        }
    }
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
    /// The package was already installed from this source: nothing was changed and
    /// `current` is the installed revision (since 0.5; installing it again used to be
    /// refused with `already_installed`, which a different source still gets).
    #[serde(
        rename = "alreadyInstalled",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub already_installed: bool,
    /// With `already_installed`: the revision the source resolved to (latest release, or
    /// the requested ref or default branch) when it differs from the installed one;
    /// `update` with the same target moves to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<Revision>,
    /// The recorded mode of the package (with `already_installed`: the mode it keeps;
    /// omitted in release mode).
    #[serde(skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
    /// With `already_installed`: the mode the install asked for when it differs from the
    /// recorded one (`update` with the same target records it).
    #[serde(rename = "requestedMode", skip_serializing_if = "Option::is_none")]
    pub requested_mode: Option<Mode>,
    /// The package is recorded enabled: false after a download (with `already_installed`
    /// or a restore: the recorded flag).
    pub enabled: bool,
    /// Missing dependencies installed (or, by a download, downloaded) from the catalog in
    /// the same transaction, sorted by ID.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<InstalledDependency>,
    /// Reconciliation report of the transaction (empty when nothing was written, for
    /// example `already_installed`, or inside an `update all` plan).
    pub dist: DistSync,
}
/// A dependency an install or download added from the catalog (see
/// [`InstallResult::dependencies`]).
#[derive(Clone, Debug, Serialize)]
pub struct InstalledDependency {
    pub id: String,
    pub source: String,
    pub current: Revision,
    /// Recorded enabled (false when downloaded).
    pub enabled: bool,
    /// Metadata keys of its manifest with the wrong type, ignored.
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
    /// `to` is a newer version outside the caret range of `from` (a new MAJOR; for 0.x
    /// a new MINOR; see [`crate::deps::is_major`]): a breaking update.
    pub major: bool,
    /// Metadata keys of the new manifest with the wrong type, ignored; for a package
    /// that stays in release mode, a moved (`release tag vV now points to COMMIT; the
    /// installed commit is kept`) or deleted (`installed release ID@V no longer exists
    /// on the origin`) installed release tag. The caller should show them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Broken release tags passed over while choosing the latest release (an update
    /// without an explicit ref; `update all` reports them in [`UpdateAllPlan::skipped`]);
    /// the caller should warn about them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<releases::SkippedTag>,
    /// Why an unchanged package stayed (for example pinned); `None` when it moved or was
    /// already at its target.
    #[serde(rename = "unchangedReason", skip_serializing_if = "Option::is_none")]
    pub unchanged_reason: Option<UnchangedReason>,
    /// The recorded mode after the operation (omitted in release mode). A change of
    /// mode alone is a change: `changed` with `from` and `to` at the same commit.
    #[serde(skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
    /// The canonical location of the verified catalog the package was detached from
    /// ([`UpdateOptions::detach`]); the record no longer has `catalog`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detached: Option<String>,
    /// Reconciliation report of the transaction (empty inside an [`UpdateAllPlan`], which
    /// carries the report of its one transaction).
    pub dist: DistSync,
}
/// The warning for a release-mode package whose installed release tag moved, broke or
/// was deleted on the origin (`found` are its releases after the fetch in `repo`); `None`
/// when the tag is where it was. Tags are not policed: the installed commit is kept.
fn release_tag_warning(
    repo: &Path,
    id: &str,
    current: &Revision,
    found: &releases::Discovery,
    custom_ssh: bool,
) -> Option<String> {
    let tag = identity::release_tag(&current.version);
    let short = |commit: &str| commit[..commit.len().min(12)].to_owned();
    if let Some(release) = found
        .tags
        .iter()
        .find(|t| t.manifest.version == current.version)
    {
        return (release.commit != current.commit).then(|| {
            format!(
                "release tag {tag} now points to {}; the installed commit is kept",
                short(&release.commit)
            )
        });
    }
    // The tag still exists but points at a commit it is no release of (a broken tag).
    if let Some(broken) = found.skipped.iter().find(|s| s.name == tag) {
        let commit = resolve_ref(repo, &format!("refs/tags/{tag}"), custom_ssh).unwrap_or_default();
        return Some(format!(
            "release tag {tag} now points to {} (skipped: {}); the installed commit is kept",
            short(&commit),
            broken.reason
        ));
    }
    Some(format!(
        "installed release {id}@{} no longer exists on the origin",
        current.version
    ))
}
/// The original `id` replaces now when `manifest` (a newer revision of `id`) no longer
/// declares that it replaces it.
fn no_longer_replaces(state: &State, id: &str, manifest: &Manifest) -> Option<String> {
    state
        .replacements
        .iter()
        .find(|(original, fork)| {
            *fork == id
                && manifest
                    .replaces
                    .as_ref()
                    .is_none_or(|r| r.id != **original)
        })
        .map(|(original, _)| original.clone())
}
/// The reason `update all` reports when it does not select `newest` of a package: the
/// newest release no longer replaces the original it replaces now, or (without a catalog)
/// it needs a dependency that is not installed. `None` means dependency constraints.
fn held_back_reason(
    state: &State,
    catalog: Option<&str>,
    newest: &semver::Version,
    versions: &BTreeMap<semver::Version, catalog::Release>,
    no_longer: Option<(semver::Version, String)>,
) -> Option<HeldBackReason> {
    if let Some((version, original)) = no_longer
        && version == *newest
    {
        return Some(HeldBackReason::NoLongerReplaces { original });
    }
    if catalog.is_some() {
        return None;
    }
    let release = versions.get(newest)?;
    release
        .manifest
        .dependencies
        .iter()
        .find(|(dependency, _)| {
            let provider = state.replacements.get(*dependency).unwrap_or(dependency);
            !state.packages.contains_key(provider)
        })
        .map(
            |(dependency, requirement)| HeldBackReason::MissingDependency {
                dependency: dependency.clone(),
                requirement: requirement.clone(),
            },
        )
}
/// `ID VERSION: plugin.json KEY must be …; ignored` for each wrong-typed metadata key.
/// Installing is lenient about metadata (only authoring refuses it).
pub(crate) fn metadata_warnings(m: &Manifest) -> Vec<String> {
    m.metadata_problems()
        .into_iter()
        .map(|problem| format!("{} {}: plugin.json {problem}; ignored", m.id, m.version))
        .collect()
}
/// Joint upgrade plan over each package's recorded mode. No packages are removed or
/// downgraded.
#[derive(Clone, Debug, Default, Serialize)]
pub struct UpdateAllPlan {
    pub updates: Vec<RevisionChange>,
    pub installs: Vec<InstallResult>,
    /// Packages for which a newer release exists than the one selected: either updated
    /// to an older one (then also in `updates`) or kept at their installed version.
    pub kept_back: Vec<HeldBack>,
    /// Installed packages that stay at their installed revision: no newer release or
    /// branch commit, or a `reason` (pinned, diverged branch history).
    /// Every installed package is in `updates`, `kept_back` or `unchanged`.
    pub unchanged: Vec<Unchanged>,
    /// Broken release tags passed over while reading the releases of every installed
    /// package (updated, kept back or unchanged), as `update ID` reports them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<releases::SkippedTag>,
    /// Reconciliation report of the transaction (`update_all`; empty for the plan).
    pub dist: DistSync,
}
/// An installed package `update all` leaves as it is: nothing newer exists, or `reason`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Unchanged {
    pub id: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<UnchangedReason>,
    /// The installed release tag moved or was deleted on the origin (see
    /// [`RevisionChange::warnings`]); the installed commit is kept.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
/// A newer release exists, but the chosen compatible solution uses an older one.
#[derive(Clone, Debug, Serialize)]
pub struct HeldBack {
    pub id: String,
    pub selected: String,
    pub latest: String,
    /// Why `latest` was not selected.
    pub reason: HeldBackReason,
}

/// What [`Manager::sync_dist`] changes (or would change) so that `dist/` holds exactly
/// the active packages. IDs are sorted.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct DistSync {
    /// Active packages whose missing `dist/<id>` is deployed from the installed commit.
    pub deployed: Vec<String>,
    /// Inactive (disabled, replaced, forgotten or retained) packages whose `dist/<id>`
    /// is removed.
    pub removed: Vec<String>,
    /// Workspace-relative paths under `dist/` that belong to no known package, or are not
    /// real folders where a namespace folder belongs; left alone.
    pub unmanaged: Vec<String>,
    /// Active packages whose missing `dist/<id>` is not deployed, with the reason (clone
    /// or installed commit missing, a blocked namespace, or another error in `sync_dist`).
    pub skipped: Vec<SkippedDist>,
    /// Actionable warnings from committed dist exports.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
/// An active package left without `dist/` (see [`DistSync::skipped`]).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SkippedDist {
    pub id: String,
    pub reason: String,
    /// The package's clone is gone (`reason` "clone missing"): only uninstalling or
    /// forgetting it (which drops the record without confirmation) ends the skip.
    pub clone_missing: bool,
    /// Link identity remains visible even when the author output is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<crate::LinkDescriptor>,
}
impl DistSync {
    pub fn is_empty(&self) -> bool {
        self.deployed.is_empty() && self.removed.is_empty()
    }
}

/// What `forget` did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ForgetResult {
    /// The clone stays and a `forgotten` record is kept (`restore` records it again);
    /// false when the clone was gone, so only the record and `dist/` were removed.
    pub clone_kept: bool,
    /// Reconciliation report.
    pub dist: DistSync,
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
    /// Installed packages that would break (their dependency resolves to this one, or
    /// to it through a replacement the original does not take over): uninstall refuses
    /// unless forced.
    pub dependents: Vec<String>,
    /// The `dependency_conflict` message uninstall raises without force when it would
    /// reactivate an original (this package replaces it) whose dependencies are not
    /// active.
    pub dependency_conflict: Option<String>,
}
impl UninstallPlan {
    /// The refusal `uninstall` raises without force, or `None`: [`Error::HasDependents`]
    /// (carrying `lost_work`) when consumers would break, else
    /// [`Error::DependencyConflict`] for a reactivated original. A caller reports it
    /// before asking about lost work, without calling `uninstall`.
    pub fn refusal(&self) -> Option<Error> {
        if !self.dependents.is_empty() {
            return Some(has_dependents(
                &self.id,
                "uninstall",
                self.dependents.clone(),
                self.lost_work.clone(),
            ));
        }
        self.dependency_conflict
            .clone()
            .map(Error::DependencyConflict)
    }
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
/// Copy the folder `path` (`""`: the root) of `commit` from `repo` into `target`: one
/// `git ls-tree`/`cat-file` of that tree (`COMMIT:PATH`, a literal path, never a pattern).
/// Plain files and folders only, the executable bit is kept; symlinks are
/// refused (a submodule comes out as an empty folder). The folder must exist.
pub(crate) fn export_folder(
    repo: &Path,
    commit: &str,
    path: &str,
    target: &Path,
    custom_ssh: bool,
) -> Result<()> {
    if !path.is_empty() {
        packages::validate_path(path)?;
    }
    let tree = format!("{commit}:{path}");
    ensure!(
        transport::object_type(repo, &tree, custom_ssh).is_ok_and(|kind| kind == "tree"),
        InvalidManifest,
        "revision {commit} has no folder {}",
        if path.is_empty() { "(root)" } else { path }
    );
    let entries = transport::list_tree(repo, &tree, true, custom_ssh)?;
    fs::create_dir_all(target)?;
    for entry in entries
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = std::str::from_utf8(entry)
            .map_err(|_| Error::InvalidData("non-UTF8 build path".into()))?;
        let (meta, name) = entry
            .split_once('\t')
            .ok_or_else(|| Error::InvalidData("invalid Git tree entry".into()))?;
        let mut meta = meta.split(' ');
        let (Some(mode), Some(kind), Some(oid)) = (meta.next(), meta.next(), meta.next()) else {
            fail!(InvalidData, "invalid Git tree entry")
        };
        packages::validate_path(name)?;
        let destination = target.join(name);
        match (mode, kind) {
            ("040000", "tree") | ("160000", "commit") => fs::create_dir_all(&destination)?,
            ("100644" | "100755", "blob") => {
                let bytes = transport::read_blob(repo, oid, custom_ssh)?;
                fs::create_dir_all(destination.parent().unwrap())?;
                fs::write(&destination, bytes)?;
                if mode == "100755" {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
                }
            }
            _ => fail!(
                Unsupported,
                "symlinks are not supported: {}",
                if path.is_empty() {
                    name.to_owned()
                } else {
                    format!("{path}/{name}")
                }
            ),
        }
    }
    Ok(())
}
/// Fill `target` with the package's declared output folder (package folder `package`, `None`: the
/// root) at `commit` of `repo`, from Git objects (never the working tree). A revision
/// without that folder is refused ([`Error::InvalidManifest`]), never deployed empty.
/// Gitlinks become empty directories; returned warnings explain their missing contents.
pub(crate) fn export_dist(
    repo: &Path,
    commit: &str,
    package: Option<&str>,
    target: &Path,
    custom_ssh: bool,
) -> Result<Vec<String>> {
    let manifest = crate::packages::manifest_at(repo, commit, package, custom_ssh)?;
    let folder = manifest.dist_path(package);
    if !transport::object_type(repo, &format!("{commit}:{folder}"), custom_ssh)
        .is_ok_and(|kind| kind == "tree")
    {
        return Err(manifest.missing_dist(format!("release {commit} has no {folder}/ folder")));
    }
    let mut warnings = Vec::new();
    let tree = format!("{commit}:{folder}");
    let entries = transport::list_tree(repo, &tree, false, custom_ssh)?;
    for entry in entries.split(|byte| *byte == 0) {
        if entry.starts_with(b"160000 ") {
            let name =
                String::from_utf8_lossy(entry.splitn(2, |byte| *byte == b'\t').nth(1).unwrap());
            warnings.push(format!("{folder}/{name} is a Git submodule; its files will not be installed and the directory will be empty. This is probably a mistake: commit built files to dist instead of a submodule."));
        }
    }
    export_folder(repo, commit, &folder, target, custom_ssh)?;
    Ok(warnings)
}
/// Locate an updated package, retaining the requested and upstream identities.
fn updated_package(
    repo: &Path,
    commit: &str,
    id: &str,
    custom_ssh: bool,
) -> Result<packages::Package> {
    packages::find_at_with_ssh(repo, commit, Some(id), custom_ssh).map_err(|error| {
        let error = match error {
            Error::InvalidArgument(message) => {
                let message = match packages::find_at_with_ssh(repo, commit, None, custom_ssh) {
                    Ok(found) => format!("package id changed upstream to {}", found.manifest.id),
                    Err(_) => format!("package id changed upstream; {message}"),
                };
                Error::invalid_manifest(message)
            }
            other => other,
        };
        error.with_package(id)
    })
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
                    "package ID {id} collides with an existing path that differs only in letter case"
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

/// `.state.json-<pid>-<counter>.tmp`, the name `write_initial_state` stages under.
fn is_initial_state_temp(name: &std::ffi::OsStr) -> bool {
    fn is_digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
    }
    name.to_str()
        .and_then(|name| name.strip_prefix(".state.json-"))
        .and_then(|id| id.strip_suffix(".tmp"))
        .and_then(|id| id.split_once('-'))
        .is_some_and(|(pid, counter)| is_digits(pid) && is_digits(counter))
}

fn write_initial_state(root: &Path) -> Result<()> {
    // Stage inside the root: the rename never crosses a filesystem, even when the root
    // is a mount point.
    crate::atomic_write::write_atomically(
        &root.join(".state.json"),
        &root.join("state.json"),
        &serde_json::to_vec_pretty(&State::default())?,
    )
}

impl Manager {
    /// A dedicated workspace only: never point this at the application's live data.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(root, Options::default())
    }
    /// Open (and create, if missing) the workspace at `root`. The root path and
    /// `options` are validated first (for example a local catalog location that does not
    /// exist is refused with [`Error::InvalidArgument`]), so a refusal creates nothing.
    pub fn open_with(root: impl AsRef<Path>, options: Options) -> Result<Self> {
        let root = std::path::absolute(root)?;
        let file_name = root
            .file_name()
            .require(Error::Workspace, "workspace needs a directory name")?;
        let parent = root
            .parent()
            .require(Error::Workspace, "workspace needs a parent")?;
        file_name
            .to_str()
            .require(Error::Workspace, "non-UTF8 workspace name")?;
        if let Some(location) = &options.catalog {
            catalog::validate_location(location)?;
        }
        txn::create_workspace_dirs(parent)?;
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
            // Another format is refused before anything is recovered or created.
            State::check_format(&fs::read(root.join("state.json"))?)?;
            txn::recover(&root)?;
        }
        if root.exists() && !root.join("state.json").exists() {
            // Under the lock, gitpm's own staged state is an abandoned write and `.DS_Store`
            // is Finder metadata, not content; anything else refuses the directory before
            // one file is removed.
            let mut abandoned = Vec::new();
            for entry in fs::read_dir(&root)? {
                let entry = entry?;
                if entry.file_name() == ".DS_Store" {
                    continue;
                }
                ensure!(
                    is_initial_state_temp(&entry.file_name()) && entry.file_type()?.is_file(),
                    Workspace,
                    "refusing nonempty directory without state.json"
                );
                abandoned.push(entry.path());
            }
            for path in abandoned {
                fs::remove_file(path)?;
            }
        }
        if !root.join("state.json").exists() {
            txn::create_workspace_dirs(&root)?;
            write_initial_state(&real)?;
        }
        fs::create_dir_all(root.join("dist"))?;
        txn::repair_worktree_links(&root, &manager.state()?)?;
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
            .require(Error::InvalidArgument, check::NO_CATALOG)
    }
    /// The workspace state (`state.json` of format [`crate::STATE_FORMAT`]).
    pub fn state(&self) -> Result<State> {
        State::parse(&fs::read(self.root.join("state.json"))?)
    }
    /// Installed packages as a flat list (sorted by ID).
    pub fn list(&self) -> Result<Vec<InstalledPackage>> {
        Ok(self.state()?.installed())
    }
    /// Run `action` on a transaction that stages only the paths it touches; nothing
    /// in the live workspace changes unless the action succeeds.
    /// Returns the action's value and the reconciliation report of `dist/`: what the
    /// transaction deployed and removed, then (read after the commit) the unmanaged paths
    /// and the active packages left without their deployed folder.
    fn transaction<T>(
        &self,
        action: impl FnOnce(&mut Txn, &mut State) -> Result<T>,
    ) -> Result<(T, DistSync)> {
        let _busy = self.busy();
        let mut txn = Txn::begin(&self.root)?;
        let mut state = self.state()?;
        let before = state.clone();
        let value = action(&mut txn, &mut state)?;
        // A build retains a source store only while its previous revision is Git.
        let mut released = BTreeMap::new();
        for (id, package) in state.packages.iter_mut().chain(&mut state.forgotten) {
            if package.build.is_some()
                && (package.previous.is_none() || package.previous_build.is_some())
                && let Some(key) = package.clone.take()
            {
                let relative = format!("repos/{key}");
                txn.remove_worktree(id, &relative)?;
                released.insert(key, id.clone());
            }
        }
        for (key, id) in released {
            if clones::clone_users(&state, &key, "").is_empty() {
                let relative = format!("repos/{key}");
                let bare = txn
                    .staged_repo(&relative)
                    .unwrap_or_else(|| self.root.join(&relative));
                if bare.is_dir() {
                    let hidden = lost_work::hidden(&bare, false, self.options.custom_ssh)?;
                    if !hidden.is_empty() {
                        return Err(lost_work::refusal(&id, hidden));
                    }
                }
                txn.delete_repo(&relative)?;
            }
        }

        settle_dist(&mut txn, &before, &state, self.options.custom_ssh)?;
        let mut report = DistSync {
            warnings: txn.warnings.clone(),
            ..Default::default()
        };
        for id in txn.staged_dists() {
            if txn.deploys_dist(&id) {
                report.deployed.push(id);
            } else if fs::symlink_metadata(self.root.join("dist").join(&id)).is_ok() {
                report.removed.push(id);
            }
        }
        // The installed revisions of each shared clone stay reachable (see `clones`): in
        // the staged copy as part of the transaction, else in the live clone afterwards.
        let keys: std::collections::BTreeSet<&str> = state
            .packages
            .values()
            .chain(state.forgotten.values())
            .filter_map(|p| p.clone.as_deref())
            .collect();
        let mut live = Vec::new();
        for key in keys {
            match txn.staged_repo(&format!("repos/{key}")) {
                Some(staged) => clones::sync_refs(&staged, &state, key, self.options.custom_ssh)?,
                None => live.push(key),
            }
        }
        txn.commit(&state)?;
        for key in live {
            let repo = self.root.join("repos").join(key);
            if repo.join("objects").is_dir() {
                // Best effort: the change is committed; the next fetch syncs again.
                let _ = clones::sync_refs(&repo, &state, key, self.options.custom_ssh);
            }
        }
        // Committed: a failing read of dist/ must not turn the change into an error.
        if let Ok(drift) = dist_drift(&self.root, &state, self.options.custom_ssh) {
            report.unmanaged = drift.unmanaged;
            report.skipped = drift.skipped;
        }
        Ok((value, report))
    }

    /// Compare installed versions with the configured catalog, or with each package's
    /// origin when there is none or it does not list the package
    /// ([`check::check`]). Read-only.
    pub fn check(&self) -> Result<Vec<CheckRow>> {
        let state = self.state()?;
        if state.packages.is_empty() {
            return Ok(Vec::new());
        }
        check::check_state(
            &state,
            self.options.catalog.as_deref(),
            self.options.custom_ssh,
        )
    }
    /// Bounded overview of the configured catalog.
    pub fn browse_catalog(&self) -> Result<catalog::CatalogOverview> {
        catalog::browse(self.catalog_location()?)
    }
    /// Exact catalog release (latest if `version` is `None`) from the configured catalog.
    pub fn catalog_release(&self, id: &str, version: Option<&str>) -> Result<catalog::Release> {
        catalog::lookup(self.catalog_location()?, id, version)
    }

    /// Install the latest release tag of a Git source (release mode).
    pub fn install(&self, source: &str) -> Result<InstallResult> {
        self.install_package(source, None, InstallTarget::Release)
    }
    /// Install `target` of a Git source and record its mode: the newest release, a ref
    /// resolved against the origin (a tag or commit pins, an origin branch is
    /// followed) or the origin's default branch (its name is recorded). A repository
    /// with a `gitpm.json` listing several packages needs `package`;
    /// for a single-package repository a given `package` must
    /// match. An install is a [`Manager::download`] plus enabling the package and the
    /// dependencies it downloaded, in one transaction: a dependency that only a disabled
    /// package provides refuses with [`Error::DependencyConflict`] ("DEP is disabled;
    /// enable it").
    pub fn install_package(
        &self,
        source: &str,
        package: Option<&str>,
        target: InstallTarget,
    ) -> Result<InstallResult> {
        self.install_package_with(source, package, target, false)
    }
    /// [`Manager::install_package`]; with `force` the package itself is enabled even when
    /// a dependency is not active (download plus a forced [`Manager::enable_with`]);
    /// missing dependencies are still installed as usual.
    pub fn install_package_with(
        &self,
        source: &str,
        package: Option<&str>,
        target: InstallTarget,
        force: bool,
    ) -> Result<InstallResult> {
        self.fetch_package(source, package, &target, Activation::Install { force })
    }
    /// Clone, verify and record a package exactly as [`Manager::install_package`] does,
    /// but disabled and without deploying it; missing dependencies are downloaded from
    /// the catalog the same way (disabled), so a later [`Manager::enable`] works offline.
    pub fn download(
        &self,
        source: &str,
        package: Option<&str>,
        target: InstallTarget,
    ) -> Result<InstallResult> {
        self.fetch_package(source, package, &target, Activation::Download)
    }
    fn fetch_package(
        &self,
        source: &str,
        package: Option<&str>,
        target: &InstallTarget,
        activation: Activation,
    ) -> Result<InstallResult> {
        if let Some(package) = package {
            valid_id(package)?;
        }
        let target = match target {
            InstallTarget::Release => FetchTarget::Latest,
            InstallTarget::Ref(reference) => FetchTarget::Ref(reference),
            InstallTarget::Dev => FetchTarget::Dev,
        };
        self.install_checked(
            Fetch {
                source,
                target,
                expected: None,
                files: None,
                package,
                path: None,
            },
            None,
            activation,
        )
    }
    /// Install an exact catalog release; manifest and dist hashes are verified.
    /// Missing dependencies come from the configured catalog.
    pub fn install_release(&self, release: &catalog::Release) -> Result<InstallResult> {
        self.install_release_from(release, None, Activation::Install { force: false })
    }
    fn install_release_from(
        &self,
        release: &catalog::Release,
        origin: Option<String>,
        activation: Activation,
    ) -> Result<InstallResult> {
        catalog::validate_release(release, &release.manifest.id, &release.manifest.version)?;
        if release.build.is_some() {
            return self.install_build_release(release, origin, activation);
        }
        self.install_checked(Fetch::release(release), origin, activation)
    }
    /// Look up `id` (latest if `version` is `None`) in the configured catalog and install
    /// it with its dependencies. The workspace works with one catalog: installing from
    /// another one than earlier catalog installs is refused.
    pub fn install_from_catalog(&self, id: &str, version: Option<&str>) -> Result<InstallResult> {
        self.install_from_catalog_with(id, version, false)
    }
    /// [`Manager::install_from_catalog`]; `force` as in [`Manager::install_package_with`].
    pub fn install_from_catalog_with(
        &self,
        id: &str,
        version: Option<&str>,
        force: bool,
    ) -> Result<InstallResult> {
        self.fetch_from_catalog(id, version, Activation::Install { force })
    }
    /// Like [`Manager::install_from_catalog`], but the package and its missing
    /// dependencies are recorded disabled and nothing is deployed (see
    /// [`Manager::download`]).
    pub fn download_from_catalog(&self, id: &str, version: Option<&str>) -> Result<InstallResult> {
        self.fetch_from_catalog(id, version, Activation::Download)
    }
    fn fetch_from_catalog(
        &self,
        id: &str,
        version: Option<&str>,
        activation: Activation,
    ) -> Result<InstallResult> {
        let release = self.catalog_release(id, version)?;
        let origin = self.protecting_catalog(&self.state()?)?;
        self.install_release_from(&release, origin, activation)
    }
    /// The canonical location of the configured catalog when it is verified: what a
    /// package installed from it records as `catalog` (it is then protected). Installing
    /// from a second verified catalog is refused with `catalog`; an unverified catalog
    /// protects nothing and is exempt (`None`).
    fn protecting_catalog(&self, state: &State) -> Result<Option<String>> {
        protecting_catalog_at(self.catalog_location()?, state)
    }
    /// Install or download one package with its missing dependencies. An install of a
    /// package downloaded (disabled) from the same source enables it.
    fn install_checked(
        &self,
        fetch: Fetch,
        origin: Option<String>,
        activation: Activation,
    ) -> Result<InstallResult> {
        let (enabled, force) = match activation {
            Activation::Download => (false, false),
            Activation::Install { force } => (true, force),
        };
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
        // Installed from this source already: report it; the transaction is dropped
        // (by returning an error out of it) so nothing is written.
        let mut already = None;
        let installed = self.transaction(|txn, state| {
            let fetch = Fetch {
                source: &source,
                ..fetch
            };
            let cloned = clone_revision(txn, &fetch, "incoming", self.options.custom_ssh)?;
            let m = cloned.manifest.clone();
            if let Some(installed) = state.packages.get(&m.id).filter(|p| p.source == source) {
                let resolved = Revision { version: m.version.clone(), commit: cloned.commit.clone(), ..Default::default() };
                // Only an explicit ref or the default branch asks for a mode.
                let requested_mode = (matches!(
                    fetch.target,
                    FetchTarget::Ref(_) | FetchTarget::Dev
                ) && cloned.mode != installed.mode)
                    .then(|| cloned.mode.clone());
                let available =
                    (resolved.commit != installed.current.commit).then_some(resolved.clone());
                // Installed from this source without protection and now asked for from a
                // verified catalog: the installed revision must be exactly that catalog's
                // release of its version; then it becomes protected.
                let protect = match (&origin, &installed.catalog) {
                    (Some(location), None) => {
                        verify_installed_release(txn, &cloned.repo, &m.id, installed, location, self.options.custom_ssh)?;
                        Some(location.clone())
                    }
                    _ => None,
                };
                let protected_note = protect
                    .as_ref()
                    .map(|location| format!("{} is now protected by catalog {location}", m.id));
                if enabled && !installed.enabled {
                    // Downloaded before: the install enables the recorded revision and mode.
                    let mut warnings: Vec<String> = protected_note.into_iter().collect();
                    if let Some(requested) = &requested_mode {
                        warnings.push(format!(
                            "{} is enabled in its recorded mode {}, not the requested {}; updating it with the same target moves it",
                            m.id,
                            mode_text(&installed.mode),
                            mode_text(requested)
                        ));
                    }
                    let result = InstallResult {
                        id: m.id.clone(),
                        source: source.clone(),
                        current: installed.current.clone(),
                        fork_of: installed.fork_of.clone(),
                        skipped: cloned.skipped.clone(),
                        warnings,
                        already_installed: false,
                        available,
                        mode: installed.mode.clone(),
                        requested_mode,
                        enabled: true,
                        dependencies: Vec::new(),
                        dist: DistSync::default(),
                    };
                    if !force && !state.replacements.contains_key(&m.id) {
                        deps::refuse_inactive_dependencies(state, &m.id, "install", true)?;
                    }
                    let entry = state.packages.get_mut(&m.id).unwrap();
                    entry.enabled = true;
                    if protect.is_some() {
                        entry.catalog = protect;
                    }
                    return Ok(result);
                }
                let result = InstallResult {
                    id: m.id.clone(),
                    source: source.clone(),
                    current: installed.current.clone(),
                    fork_of: installed.fork_of.clone(),
                    skipped: cloned.skipped.clone(),
                    warnings: protected_note.into_iter().collect(),
                    already_installed: true,
                    available,
                    mode: installed.mode.clone(),
                    requested_mode,
                    enabled: installed.enabled,
                    dependencies: Vec::new(),
                    dist: DistSync::default(),
                };
                if protect.is_some() {
                    // Recording the protection is the only change.
                    state.packages.get_mut(&m.id).unwrap().catalog = protect;
                    return Ok(result);
                }
                already = Some(result);
            }
            check_new_id(txn.root(), state, &m)?;
            // Everything installed stays at its version; the new package is added on top.
            // A download may take the versions of disabled packages as given; an install
            // refuses them below, after the solve.
            let mut next = state.clone();
            next.packages.insert(m.id.clone(), Installed::default());
            next.packages.get_mut(&m.id).unwrap().current.version = m.version.clone();
            next.packages.get_mut(&m.id).unwrap().adopt(&m);
            let mut roots = deps::healthy_roots(state)?;
            if !state.replacements.contains_key(&m.id) {
                roots.insert(m.id.clone(), deps::version(&m.version)?);
            }
            let mut fixed = deps::fixed_from_state(&next)?;
            if let Some(new) = fixed.get_mut(&m.id) {
                new.note = Some(format!("being installed at {}", m.version));
            }
            let missing = deps::solve(&fixed, &roots, self.options.catalog.as_deref())?;
            let dependencies = self.install_missing(
                txn,
                state,
                &missing,
                enabled,
                self.options.catalog.as_deref(),
            )?;
            let skipped = cloned.skipped.clone();
            let mut result = adopt(
                txn,
                state,
                cloned,
                source.clone(),
                origin.clone(),
                enabled,
                self.options.custom_ssh,
            )?;
            if enabled && !force && state.is_active(&m.id) {
                deps::refuse_inactive_dependencies(state, &m.id, "install", true)?;
            }
            result.skipped = skipped;
            result.dependencies = dependencies;
            Ok(result)
        });
        match (installed, already) {
            (Err(Error::AlreadyInstalled { .. }), Some(already)) => Ok(already),
            (installed, _) => installed.map(|(result, dist)| InstallResult { dist, ..result }),
        }
    }
    /// Install catalog releases chosen by the solver (inside the current transaction)
    /// from the catalog at `location`, recorded `enabled` or disabled (download), and
    /// protected when that catalog is verified.
    fn install_missing(
        &self,
        txn: &mut Txn,
        state: &mut State,
        missing: &BTreeMap<String, semver::Version>,
        enabled: bool,
        location: Option<&str>,
    ) -> Result<Vec<InstalledDependency>> {
        if missing.is_empty() {
            return Ok(Vec::new());
        }
        let location = location.require(Error::InvalidArgument, check::NO_CATALOG)?;
        let origin = protecting_catalog_at(location, state)?;
        let mut installed = Vec::new();
        for (number, (id, version)) in missing.iter().enumerate() {
            let release = catalog::lookup(location, id, Some(&version.to_string()))
                .and_then(|release| {
                    catalog::validate_release(&release, id, &release.manifest.version)?;
                    Ok(release)
                })
                .map_err(|error| error.with_package(id))?;
            let result = if release.build.is_some() {
                builds::adopt_build(
                    txn,
                    state,
                    &release,
                    Some(catalog::canonical_location(location)?),
                    origin.clone(),
                    enabled,
                )?
            } else {
                install_pulled(
                    txn,
                    state,
                    id,
                    &release,
                    &format!("dependency-{number}"),
                    origin.clone(),
                    enabled,
                    self.options.custom_ssh,
                )?
            };
            installed.push(InstalledDependency {
                id: result.id,
                source: result.source,
                current: result.current,
                enabled: result.enabled,
                warnings: result.warnings,
            });
        }
        Ok(installed)
    }
    /// Check that `id` moving to manifest `m` keeps every working dependency working;
    /// with a `catalog`, dependencies the new revision adds are installed from it.
    fn check_change(
        &self,
        txn: &mut Txn,
        state: &mut State,
        id: &str,
        m: &Manifest,
        catalog: Option<&str>,
    ) -> Result<()> {
        for (original, fork) in &state.replacements {
            if fork == id && m.replaces.as_ref().is_none_or(|r| r.id != *original) {
                return Err(Error::version_conflict(format!(
                    "version {} no longer declares that it replaces {original}, which it replaces now",
                    m.version
                ))
                .with_package(id));
            }
        }
        let mut next = state.clone();
        let entry = next
            .packages
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
                    deps::version(&next.packages[&root].current.version)?,
                );
            }
        }
        let mut fixed = deps::fixed_from_state(&next)?;
        if let Some(changed) = fixed.get_mut(id) {
            changed.note = Some(format!("being changed to {}", m.version));
        }
        let missing = deps::solve(&fixed, &roots, catalog)?;
        // Dependencies an inactive package pulls stay disabled with it.
        let active = state.is_active(id);
        self.install_missing(txn, state, &missing, active, catalog)
            .map(drop)
    }
    /// Resolve all installed packages together, without changing installed files or state.
    /// Reads release tags from each origin; new dependencies come from the catalog.
    /// Prefers newer versions, with installed IDs in lexical order as tie-breaker.
    pub fn update_all_plan(&self) -> Result<UpdateAllPlan> {
        let _busy = self.busy();
        let mut txn = Txn::begin(&self.root)?;
        let mut state = self.state()?;
        self.prepare_update_all(&mut txn, &mut state)
            .map_err(Error::listing_lost_work)
    }

    /// Resolve and apply a joint upgrade in one recoverable transaction.
    /// Local work blocks the operation. No merge, deletion or downgrade is attempted.
    pub fn update_all(&self) -> Result<UpdateAllPlan> {
        let (plan, dist) = self
            .transaction(|txn, state| self.prepare_update_all(txn, state))
            .map_err(Error::listing_lost_work)?;
        Ok(UpdateAllPlan { dist, ..plan })
    }

    fn prepare_update_all(&self, txn: &mut Txn, state: &mut State) -> Result<UpdateAllPlan> {
        let mut candidates = crate::upgrade::Candidates::new();
        let mut latest = BTreeMap::new();
        // One scratch copy per clone: the packages of a multi-package source share it.
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, installed) in &state.packages {
            if installed.build.is_some() || installed.link().is_some() {
                continue;
            }
            groups
                .entry(installed.clone_folder(id))
                .or_default()
                .push(id.clone());
        }
        let mut repos: BTreeMap<String, PathBuf> = BTreeMap::new();
        // Packages that stay by their mode: pinned, or a diverged branch history.
        let mut reasons: BTreeMap<String, UnchangedReason> = BTreeMap::new();
        // Broken consumers (after a forced removal, say) stay at their installed revision
        // and their unsatisfied dependencies stay out of the solve, so a removed
        // dependency is never installed again for them and never fails the run.
        let mut broken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for id in state.packages.keys() {
            let rows = deps::broken_dependencies(state, id);
            if let Some(first) = rows.first() {
                reasons.insert(
                    id.clone(),
                    UnchangedReason::BrokenDependency {
                        dependency: first.id.clone(),
                        problem: first.problem.clone().unwrap_or_default(),
                    },
                );
                broken.insert(id.clone(), rows.into_iter().map(|row| row.id).collect());
            }
        }
        // Why a package whose newest release is not selected is kept back, when it is
        // more than dependency constraints.
        let mut held: BTreeMap<String, HeldBackReason> = BTreeMap::new();
        // Release-mode packages whose installed release tag moved or was deleted.
        let mut tag_warnings: BTreeMap<String, String> = BTreeMap::new();
        // With protected packages, new dependencies come from their verified catalog (one
        // per workspace) while it is still verified, whatever the configured catalog is.
        let dependency_catalog = state
            .packages
            .values()
            .find_map(|p| p.catalog.clone())
            .filter(|location| catalog::is_verified(location).unwrap_or(false))
            .or_else(|| {
                state
                    .packages
                    .values()
                    .find_map(|p| p.build.as_ref().and_then(|b| b.location.clone()))
            })
            .or_else(|| self.options.catalog.clone());
        let catalog = dependency_catalog.as_deref();
        // Broken release tags passed over, for every installed package.
        let mut skipped: Vec<releases::SkippedTag> = Vec::new();
        let installed_versions: Vec<(String, String)> = state
            .packages
            .iter()
            .map(|(id, p)| (id.clone(), p.current.version.clone()))
            .collect();
        for (id, p) in &state.packages {
            let Some(build) = &p.build else {
                continue;
            };
            let current = deps::version(&p.current.version)?;
            let mut versions = BTreeMap::from([(current.clone(), build.release.clone())]);
            let mut newest = current;
            let mut no_longer = None;
            if !broken.contains_key(id) {
                let location = build
                    .location
                    .as_deref()
                    .require(Error::InvalidArgument, check::NO_CATALOG)?;
                match builds::candidates(p, id, location) {
                    Ok(found) => {
                        for release in found {
                            let key = deps::version(&release.manifest.version)?;
                            newest = newest.max(key.clone());
                            if let Some(original) = no_longer_replaces(state, id, &release.manifest)
                            {
                                if no_longer.as_ref().is_none_or(|(v, _)| *v < key) {
                                    no_longer = Some((key, original));
                                }
                                continue;
                            }
                            versions.insert(key, release);
                        }
                    }
                    Err(error) => {
                        reasons.insert(
                            id.clone(),
                            UnchangedReason::CatalogUnavailable {
                                reason: error.to_string(),
                            },
                        );
                    }
                }
            }
            if let Some(reason) = held_back_reason(state, catalog, &newest, &versions, no_longer) {
                held.insert(id.clone(), reason);
            }
            latest.insert(id.clone(), newest.to_string());
            candidates.insert(id.clone(), versions);
        }
        for (number, (relative, ids)) in groups.iter().enumerate() {
            let first = &ids[0];
            let key = state.packages[first].clone.as_deref();
            for id in ids {
                let live = installed_repo(txn.root(), id, &state.packages[id])?;
                guard_clone(
                    &live,
                    state,
                    id,
                    "local commits detected; update would discard them",
                    "unpublished local work blocks update all",
                    true,
                    self.options.custom_ssh,
                )
                .map_err(|error| error.with_package(id))?;
            }
            let live = source_repo(txn.root(), first, &state.packages[first])?;
            // Scratch copies ensure even held-back repositories keep their original refs.
            let repo = txn.scratch(&format!("upgrade-{number}"));
            copy_tree(&live, &repo, true)?;
            match key {
                Some(key) => clones::fetch_tags(&repo, state, key, true, self.options.custom_ssh)?,
                None => clones::fetch_origin(&repo, true, self.options.custom_ssh)?,
            }
            for id in ids {
                let installed = &state.packages[id];
                let current = deps::version(&installed.current.version)?;
                let mut newest = current.clone();
                let mut versions = BTreeMap::new();
                // Read at the installed commit, from the recorded package folder: a shared
                // source's bare HEAD does not identify a package revision, and a root package may
                // have moved into a package folder since.
                let current_manifest = packages::find_at_with_ssh(
                    &repo,
                    &installed.current.commit,
                    Some(id),
                    self.options.custom_ssh,
                )
                .map_err(|error| error.with_package(id))?
                .manifest;
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
                if broken.contains_key(id) {
                    latest.insert(id.clone(), newest.to_string());
                    candidates.insert(id.clone(), versions);
                    continue;
                }
                // The newest release dropped because it no longer declares that it
                // replaces the original this package replaces now.
                let mut no_longer: Option<(semver::Version, String)> = None;
                // Only release mode reads release tags: a pinned package keeps its
                // revision, a branch-mode package gets its branch HEAD as only candidate.
                match &installed.mode {
                    Mode::Release => {}
                    Mode::Link(_) => unreachable!("links are fixed candidates"),
                    Mode::Pinned(reference) => {
                        reasons.insert(
                            id.clone(),
                            UnchangedReason::Pinned {
                                reference: reference.clone(),
                            },
                        );
                        latest.insert(id.clone(), newest.to_string());
                        candidates.insert(id.clone(), versions);
                        continue;
                    }
                    Mode::Branch(branch) => {
                        let Ok(head) = resolve_ref(
                            &repo,
                            &format!("refs/remotes/origin/{branch}"),
                            self.options.custom_ssh,
                        ) else {
                            reasons.insert(
                                id.clone(),
                                UnchangedReason::BranchGone {
                                    branch: branch.clone(),
                                },
                            );
                            latest.insert(id.clone(), newest.to_string());
                            candidates.insert(id.clone(), versions);
                            continue;
                        };
                        if head != installed.current.commit {
                            let ancestor = transport::require_ancestor(
                                &repo,
                                &installed.current.commit,
                                &head,
                                self.options.custom_ssh,
                            );
                            if ancestor.is_err() {
                                reasons.insert(
                                    id.clone(),
                                    UnchangedReason::Diverged {
                                        branch: branch.clone(),
                                    },
                                );
                            } else {
                                let found =
                                    updated_package(&repo, &head, id, self.options.custom_ssh)?;
                                let version = deps::version(&found.manifest.version)?;
                                let dropped = no_longer_replaces(state, id, &found.manifest);
                                if let Some(original) = dropped.clone()
                                    && version > current
                                {
                                    newest = newest.max(version.clone());
                                    no_longer = Some((version.clone(), original));
                                }
                                // The branch commit is the identity: at the installed
                                // version it takes the installed revision's place.
                                if version >= current && dropped.is_none() {
                                    deps::validate(&found.manifest)
                                        .map_err(|error| error.with_package(id))?;
                                    newest = newest.max(version.clone());
                                    versions.insert(
                                        version,
                                        catalog::Release {
                                            path: found.subdir().map(str::to_owned),
                                            manifest: found.manifest,
                                            source: installed.source.clone(),
                                            commit: head,
                                            reference: branch.clone(),
                                            files: BTreeMap::new(),
                                            build: None,
                                        },
                                    );
                                }
                            }
                        }
                        if let Some(reason) =
                            held_back_reason(state, catalog, &newest, &versions, no_longer)
                        {
                            held.insert(id.clone(), reason);
                        }
                        latest.insert(id.clone(), newest.to_string());
                        candidates.insert(id.clone(), versions);
                        continue;
                    }
                }
                // A protected package: only the versions its verified catalog lists.
                if let Some(location) = &installed.catalog {
                    // An unusable catalog keeps only this package where it is.
                    let (releases, upstream) = match catalog_candidates(
                        &repo,
                        id,
                        installed,
                        location,
                        self.options.custom_ssh,
                    ) {
                        Ok(found) => found,
                        Err(error) => {
                            reasons.insert(
                                id.clone(),
                                UnchangedReason::CatalogUnavailable {
                                    reason: error.to_string(),
                                },
                            );
                            latest.insert(id.clone(), newest.to_string());
                            candidates.insert(id.clone(), versions);
                            continue;
                        }
                    };
                    if releases.is_empty()
                        && let Some(version) = upstream
                    {
                        reasons.insert(id.clone(), UnchangedReason::NotInCatalog { version });
                    }
                    for release in releases {
                        let version = deps::version(&release.manifest.version)?;
                        newest = newest.max(version.clone());
                        if let Some(original) = no_longer_replaces(state, id, &release.manifest) {
                            if no_longer.as_ref().is_none_or(|(v, _)| *v < version) {
                                no_longer = Some((version, original));
                            }
                            continue;
                        }
                        deps::validate(&release.manifest)
                            .map_err(|error| error.with_package(id))?;
                        versions.insert(version, release);
                    }
                    if let Some(reason) =
                        held_back_reason(state, catalog, &newest, &versions, no_longer)
                    {
                        held.insert(id.clone(), reason);
                    }
                    latest.insert(id.clone(), newest.to_string());
                    candidates.insert(id.clone(), versions);
                    continue;
                }
                // Every tag of the package may be gone (all retracted).
                let found =
                    match releases::discover_all_with_ssh(&repo, Some(id), self.options.custom_ssh)
                    {
                        Err(Error::NoReleases(_)) => releases::Discovery::default(),
                        other => other?,
                    };
                if let Some(warning) = release_tag_warning(
                    &repo,
                    id,
                    &installed.current,
                    &found,
                    self.options.custom_ssh,
                ) {
                    tag_warnings.insert(id.clone(), warning);
                }
                // Pre-releases only on the installed pre-release's track.
                let mut found = found;
                found.offered(Some(&current));
                skipped.extend(found.skipped);
                for tag in found.tags {
                    let version = deps::version(&tag.manifest.version)?;
                    // Versions only: the installed revision (kept by commit) stands for
                    // its version, also when the tag moved.
                    if version <= current {
                        continue;
                    }
                    newest = newest.max(version.clone());
                    // An active replacement must continue providing the same original ID.
                    if let Some(original) = no_longer_replaces(state, id, &tag.manifest) {
                        if no_longer.as_ref().is_none_or(|(v, _)| *v < version) {
                            no_longer = Some((version, original));
                        }
                        continue;
                    }
                    deps::validate(&tag.manifest).map_err(|error| error.with_package(id))?;
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
                if let Some(reason) =
                    held_back_reason(state, catalog, &newest, &versions, no_longer)
                {
                    held.insert(id.clone(), reason);
                }
                latest.insert(id.clone(), newest.to_string());
                candidates.insert(id.clone(), versions);
            }
            repos.insert(relative.clone(), repo);
        }
        // Links contribute a single fixed snapshot to the joint solve. Discovery
        // is read-only and its outcome cannot change that candidate.
        let link_rows: BTreeMap<_, _> = if state.packages.values().any(|p| p.link().is_some()) {
            check::check_with_options(txn.root(), self.options.catalog.as_deref(), &self.options)?
                .rows
        } else {
            Vec::new()
        }
        .into_iter()
        .filter(|r| matches!(r.mode, Mode::Link(_)))
        .map(|r| (r.id.clone(), r))
        .collect();
        for (id, p) in &state.packages {
            if p.link().is_none() {
                continue;
            }
            let version = deps::version(&p.current.version)?;
            let m = Manifest {
                id: id.clone(),
                version: p.current.version.clone(),
                dependencies: p.dependencies.clone(),
                replaces: p.replaces.clone(),
                ..Default::default()
            };
            candidates.insert(
                id.clone(),
                BTreeMap::from([(
                    version,
                    catalog::Release {
                        manifest: m,
                        source: p.source.clone(),
                        commit: String::new(),
                        reference: String::new(),
                        files: BTreeMap::new(),
                        build: None,
                        path: None,
                    },
                )]),
            );
            latest.insert(id.clone(), p.current.version.clone());
            reasons.insert(
                id.clone(),
                UnchangedReason::DevLinked {
                    availability: Box::new(link_rows[id].clone()),
                    dependency_problems: deps::broken_dependencies(state, id)
                        .into_iter()
                        .map(|d| format!("{}: {}", d.id, d.problem.unwrap_or_default()))
                        .collect(),
                },
            );
        }
        let crate::upgrade::Solution {
            selected,
            active_new,
        } = crate::upgrade::solve(state, &broken, candidates, catalog)?;
        let mut plan = UpdateAllPlan::default();
        for (number, (id, release)) in selected.iter().enumerate() {
            let m = &release.manifest;
            if let Some(installed) = state.packages.get(id) {
                if latest[id] != m.version {
                    plan.kept_back.push(HeldBack {
                        id: id.clone(),
                        selected: m.version.clone(),
                        latest: latest[id].clone(),
                        reason: held.remove(id).unwrap_or(HeldBackReason::Constraints),
                    });
                }
                if release.commit == installed.current.commit {
                    continue;
                }
                if installed.build.is_some() || release.build.is_some() {
                    let location = installed
                        .catalog
                        .clone()
                        .or_else(|| installed.build.as_ref().and_then(|b| b.location.clone()))
                        .or_else(|| dependency_catalog.clone());
                    plan.updates
                        .push(self.apply_catalog_revision(txn, state, id, release, location)?);
                    continue;
                }
                let relative = installed.clone_folder(id);
                let repo = match txn.staged_repo(&relative) {
                    Some(staged) => staged,
                    None => repos[&relative].clone(),
                };
                // A protected package's catalog release is verified before any change.
                let protected = installed.catalog.is_some();
                if protected {
                    ensure_release_commit(&repo, release, self.options.custom_ssh)?;
                    let found =
                        updated_package(&repo, &release.commit, id, self.options.custom_ssh)?;
                    ensure!(
                        found.subdir() == release.path.as_deref(),
                        Catalog,
                        "catalog package folder {} of {id}@{} does not match git revision ({})",
                        release.path.as_deref().unwrap_or("(root)"),
                        m.version,
                        found.subdir().unwrap_or("(root)")
                    );
                }
                if txn.staged_repo(&relative).is_none() {
                    txn.add_repo(&relative, &repo)?;
                }
                let checkout =
                    txn.worktree(id, &relative, &release.commit, self.options.custom_ssh)?;
                let actual = package_manifest(&checkout, release.path.as_deref())
                    .map_err(|error| error.with_package(id))?;
                if protected {
                    verify_catalog_manifest(&actual, release)?;
                }
                if serde_json::to_value(&actual)? != serde_json::to_value(m)? {
                    return Err(
                        Error::version_conflict("release manifest changed".into()).with_package(id)
                    );
                }
                let next = revision(&checkout, m, self.options.custom_ssh)?;
                let installed = state.packages.get_mut(id).unwrap();
                plan.updates.push(RevisionChange {
                    id: id.clone(),
                    from: installed.current.clone(),
                    to: next.clone(),
                    changed: true,
                    major: deps::is_major_text(&installed.current.version, &next.version),
                    warnings: metadata_warnings(m),
                    skipped: Vec::new(),
                    unchanged_reason: None,
                    mode: installed.mode.clone(),
                    detached: None,
                    dist: DistSync::default(),
                });
                installed.previous = Some(installed.current.with_mode(&installed.mode));
                installed.current = next;
                installed.path = release.path.clone();
                installed.adopt(m);
                let path = installed.path.clone();
                if txn.staged_repo(&relative).is_none() {
                    txn.add_repo(&relative, &repo)?;
                }
                let staged = txn
                    .deploy(
                        id,
                        &relative,
                        path.as_deref(),
                        &release.commit,
                        self.options.custom_ssh,
                    )
                    .map_err(|error| error.with_package(id))?;
                if protected {
                    verify_catalog_release(&actual, release, &staged)?;
                }
            } else {
                let origin = protecting_catalog_at(
                    catalog.require(Error::InvalidArgument, check::NO_CATALOG)?,
                    state,
                )?;
                if release.build.is_some() {
                    plan.installs.push(builds::adopt_build(
                        txn,
                        state,
                        release,
                        dependency_catalog.clone(),
                        origin,
                        active_new.contains(id),
                    )?);
                    continue;
                }
                plan.installs.push(install_pulled(
                    txn,
                    state,
                    id,
                    release,
                    &format!("upgrade-new-{number}"),
                    origin,
                    // Enabled only when an active package pulls it (as `update ID`).
                    active_new.contains(id),
                    self.options.custom_ssh,
                )?);
            }
        }
        skipped.sort_by(|a, b| a.name.cmp(&b.name));
        plan.skipped = skipped;
        for (id, version) in installed_versions {
            if !plan.updates.iter().any(|c| c.id == id)
                && !plan.kept_back.iter().any(|h| h.id == id)
            {
                let reason = reasons.remove(&id);
                let warnings = tag_warnings.remove(&id).into_iter().collect();
                plan.unchanged.push(Unchanged {
                    id,
                    version,
                    reason,
                    warnings,
                });
            }
        }
        Ok(plan)
    }

    /// [`Manager::update_to`] following the recorded mode ([`Target::Recorded`]).
    pub fn update(&self, id: &str) -> Result<RevisionChange> {
        self.update_to(id, Target::Recorded)
    }
    /// Move `id` by `target` (see [`Target`]): the recorded mode, back to releases, a ref
    /// (a tag or commit pins, an origin branch is followed) or the origin's default
    /// branch; the new mode is recorded with the revision. A pinned package without an
    /// explicit target stays unchanged with [`UnchangedReason::Pinned`]. Branch mode
    /// moves by commit and refuses a diverged history; a tag or commit must change the
    /// version. Refused if the new version breaks a dependent; new dependencies are
    /// installed. A package protected by a verified catalog moves only to the versions
    /// that catalog lists (see [`Manager::update_with`]).
    pub fn update_to(&self, id: &str, target: Target) -> Result<RevisionChange> {
        self.update_with(id, target, UpdateOptions::default())
    }
    /// [`Manager::update_to`] with [`UpdateOptions`]. A package protected by a verified
    /// catalog (recorded `catalog`) follows releases only through that catalog, read from
    /// the recorded location whatever [`Options::catalog`] is: it moves to the newest
    /// version the catalog lists above the installed one, after checking that the
    /// origin has the release commit and that its manifest, package folder and dist file
    /// hashes equal the release record (`catalog` otherwise, nothing changed). A newer
    /// origin release the catalog lacks leaves it unchanged with
    /// [`UnchangedReason::NotInCatalog`]; a ref or branch target is refused with
    /// `invalid_argument`. `detach` drops the protection first and then updates from the
    /// origin by `target`, in one transaction.
    pub fn update_with(
        &self,
        id: &str,
        target: Target,
        options: UpdateOptions,
    ) -> Result<RevisionChange> {
        valid_id(id)?;
        let ssh = self.options.custom_ssh;
        let (change, dist) = self
            .transaction(|txn, state| {
                let p = state
                    .packages
                    .get(id)
                    .cloned()
                    .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
                if p.link().is_some() {
                    let row = check::check_with_options(
                        txn.root(),
                        self.options.catalog.as_deref(),
                        &self.options,
                    )?
                    .rows
                    .into_iter()
                    .find(|r| r.id == id)
                    .unwrap();
                    return Ok(RevisionChange {
                        id: id.into(),
                        from: p.current.clone(),
                        to: p.current.clone(),
                        changed: false,
                        major: false,
                        warnings: Vec::new(),
                        skipped: Vec::new(),
                        unchanged_reason: Some(UnchangedReason::DevLinked {
                            availability: Box::new(row),
                            dependency_problems: deps::broken_dependencies(state, id)
                                .into_iter()
                                .map(|d| format!("{}: {}", d.id, d.problem.unwrap_or_default()))
                                .collect(),
                        }),
                        mode: p.mode.clone(),
                        detached: None,
                        dist: DistSync::default(),
                    });
                }
                let unchanged = |reason: Option<UnchangedReason>, skipped| RevisionChange {
                    id: id.into(),
                    from: p.current.clone(),
                    to: p.current.clone(),
                    changed: false,
                    major: false,
                    warnings: Vec::new(),
                    skipped,
                    unchanged_reason: reason,
                    mode: p.mode.clone(),
                    detached: None,
                    dist: DistSync::default(),
                };
                if !options.detach && matches!(target, Target::Recorded | Target::Release) {
                    let location = p
                        .catalog
                        .clone()
                        .or_else(|| p.build.as_ref().and_then(|b| b.location.clone()));
                    if let Some(location) = location {
                        let listed = builds::candidates(&p, id, &location)?;
                        let release = listed
                            .last()
                            .filter(|r| r.build.is_some() || p.build.is_some());
                        if let Some(release) = release {
                            if p.build.is_none() {
                                let live = installed_repo(txn.root(), id, &p)?;
                                guard_clone(
                                    &live,
                                    state,
                                    id,
                                    "local commits detected",
                                    "unpublished work blocks update",
                                    true,
                                    ssh,
                                )?;
                            }
                            self.check_change(txn, state, id, &release.manifest, Some(&location))?;
                            let result = self.apply_catalog_revision(
                                txn,
                                state,
                                id,
                                release,
                                Some(location),
                            )?;
                            if state.is_active(id) {
                                deps::refuse_inactive_dependencies(state, id, "update", false)?;
                            }
                            return Ok(result);
                        }
                        if p.build.is_some() {
                            return Ok(unchanged(None, Vec::new()));
                        }
                    }
                }
                let transitioning_build = p.build.is_some();
                if transitioning_build {
                    let mut retained = p.clone();
                    retained
                        .clone
                        .get_or_insert_with(|| clones::repo_key(&p.source));
                    let relative = retained.clone_folder(id);
                    match source_repo(txn.root(), id, &retained) {
                        Ok(live) => {
                            lost_work::require_no_hidden_for_update(
                                &live,
                                id,
                                "unpublished work blocks update",
                                ssh,
                            )?;
                            txn.repo(&relative, &live)?;
                        }
                        Err(Error::RepositoryNotFound { .. }) => {
                            let scratch = txn.scratch("build-transition");
                            fs::create_dir_all(scratch.parent().unwrap())?;
                            transport::clone_bare(
                                scratch.parent().unwrap(),
                                &p.source,
                                "build-transition",
                                ssh,
                            )?;
                            lost_work::record_origin_tags(&scratch, ssh)?;
                            txn.add_repo(&relative, &scratch)?;
                        }
                        Err(error) => return Err(error),
                    }
                    state.packages.get_mut(id).unwrap().clone = retained.clone;
                }
                let detached = if options.detach {
                    let location = p
                        .catalog
                        .clone()
                        .or_else(|| p.build.as_ref().and_then(|b| b.location.clone()))
                        .ok_or_else(|| {
                            Error::InvalidArgument(format!(
                                "{id} is not protected by a catalog; there is nothing to detach"
                            ))
                        })?;
                    state.packages.get_mut(id).unwrap().catalog = None;
                    Some(location)
                } else {
                    None
                };
                let protected = p
                    .catalog
                    .clone()
                    .filter(|_| detached.is_none() && !transitioning_build);
                if let Some(location) = &protected {
                    ensure!(
                        matches!(target, Target::Recorded | Target::Release),
                        InvalidArgument,
                        "{id} is protected by catalog {location} and moves only to the versions \
                         it lists; detach it to follow a ref or branch of its origin"
                    );
                }
                if let (Target::Recorded, Mode::Pinned(reference), None) =
                    (&target, &p.mode, &detached)
                {
                    return Ok(unchanged(
                        Some(UnchangedReason::Pinned {
                            reference: reference.clone(),
                        }),
                        Vec::new(),
                    ));
                }
                let relative = p.clone_folder(id);
                if !transitioning_build {
                    let live = installed_repo(txn.root(), id, &p)?;
                    guard_clone(
                        &live,
                        state,
                        id,
                        "local commits detected; update would discard them",
                        "local branches, tags or stash hold unpublished work; \
                         keep or remove them before updating",
                        true,
                        ssh,
                    )?;
                }
                let repo = match txn.staged_repo(&relative) {
                    Some(staged) => staged,
                    None => {
                        let live = source_repo(txn.root(), id, &p)?;
                        txn.repo(&relative, &live)?
                    }
                };
                // Mirror the origin's tags exactly: a deleted release disappears, a moved one
                // is seen at its new commit (tags are not policed; the installed revision is
                // kept by commit). Local tags holding unpublished commits were refused above.
                match p.clone.as_deref() {
                    Some(key) => {
                        clones::fetch_tags(&repo, state, key, true, self.options.custom_ssh)?
                    }
                    None => clones::fetch_origin(&repo, true, self.options.custom_ssh)?,
                }
                if matches!(&target, Target::Dev) || target == Target::Ref("HEAD".into()) {
                    crate::transport::set_remote_head(&repo, "origin", self.options.custom_ssh)?;
                }
                let mut skipped = Vec::new();
                // Moved or retracted installed release tag (release mode).
                let mut warnings: Vec<String> = Vec::new();
                // The catalog release a protected package moves to.
                let mut expected: Option<catalog::Release> = None;
                let (next_commit, mode) = match (&target, &p.mode) {
                    _ if protected.is_some() => {
                        let location = protected.as_deref().unwrap();
                        let (releases, upstream) =
                            catalog_candidates(&repo, id, &p, location, ssh)?;
                        let Some(release) = releases.into_iter().last() else {
                            return Ok(unchanged(
                                upstream.map(|version| UnchangedReason::NotInCatalog { version }),
                                Vec::new(),
                            ));
                        };
                        let commit = release.commit.clone();
                        ensure_release_commit(&repo, &release, self.options.custom_ssh)?;
                        expected = Some(release);
                        (commit, Mode::Release)
                    }
                    (Target::Ref(reference), _) => resolve_upstream_ref(&repo, reference, ssh)?,
                    (Target::Dev, _) => {
                        resolve_upstream_ref(&repo, "HEAD", self.options.custom_ssh)?
                    }
                    (Target::Recorded, Mode::Branch(branch)) => {
                        let commit =
                            resolve_ref(&repo, &format!("refs/remotes/origin/{branch}"), ssh)
                                .map_err(|_| {
                                    Error::version_conflict(format!(
                                        "the origin no longer has the branch {branch} it follows; \
                                         automatic update refused"
                                    ))
                                })?;
                        (commit, p.mode.clone())
                    }
                    _ => {
                        // Following releases (no explicit target) compares versions only.
                        let following = target == Target::Recorded;
                        let mut found = match releases::discover_all_with_ssh(&repo, Some(id), ssh)
                        {
                            // Every release tag was deleted on the origin: the installed one
                            // was retracted, the package stays.
                            Err(Error::NoReleases(_)) if following => {
                                releases::Discovery::default()
                            }
                            other => other?,
                        };
                        let current_version = catalog::version_key(&p.current.version)?;
                        // The installed release tag is looked up among all of them.
                        let tag_warning = release_tag_warning(&repo, id, &p.current, &found, ssh);
                        // Pre-releases only on the installed pre-release's track.
                        found.offered(Some(&current_version));
                        if !found.tags.is_empty() {
                            found.ensure_latest_is_valid()?;
                        }
                        skipped = std::mem::take(&mut found.skipped);
                        let latest = found.tags.pop();
                        let latest_version = latest
                            .as_ref()
                            .map(|tag| catalog::version_key(&tag.manifest.version))
                            .transpose()?;
                        match latest {
                            // Following releases never downgrades: a retracted installed
                            // release stays (an explicit return to releases may go down).
                            _ if following && latest_version.as_ref() < Some(&current_version) => {
                                warnings.extend(tag_warning);
                                (p.current.commit.clone(), Mode::Release)
                            }
                            // The tag of the installed version moved: the installed commit
                            // is kept (a branch commit may carry the version of the release
                            // before it, and then moves to the release).
                            Some(_)
                                if !matches!(p.mode, Mode::Branch(_))
                                    && latest_version.as_ref() == Some(&current_version) =>
                            {
                                warnings.extend(tag_warning);
                                (p.current.commit.clone(), Mode::Release)
                            }
                            Some(latest) => (latest.commit, Mode::Release),
                            None => fail!(NoReleases, "no releases"),
                        }
                    }
                };
                // Following the recorded branch (no explicit ref): its history must contain
                // the installed commit.
                let following = matches!(&mode, Mode::Branch(_))
                    && mode == p.mode
                    && !matches!(target, Target::Ref(_));
                if next_commit == p.current.commit && !transitioning_build {
                    // A new mode (or a detach) is recorded even when the revision stays: that
                    // is a change.
                    if mode == p.mode && detached.is_none() && !transitioning_build {
                        return Ok(RevisionChange {
                            warnings,
                            ..unchanged(None, skipped)
                        });
                    }
                    state.packages.get_mut(id).unwrap().mode = mode.clone();
                    return Ok(RevisionChange {
                        changed: true,
                        mode,
                        warnings,
                        detached,
                        ..unchanged(None, skipped)
                    });
                }
                if following {
                    let Mode::Branch(branch) = &mode else {
                        unreachable!()
                    };
                    transport::require_ancestor(
                        &repo,
                        &p.current.commit,
                        &next_commit,
                        self.options.custom_ssh,
                    )
                    .context(
                        Error::version_conflict,
                        format!("history of {branch} diverged; automatic update refused"),
                    )?;
                }
                // The package can move to another folder (or into a multi-package layout).
                let path = updated_package(&repo, &next_commit, id, ssh)?
                    .subdir()
                    .map(str::to_owned);
                if let Some(release) = &expected {
                    ensure!(
                        path == release.path,
                        Catalog,
                        "catalog package folder {} of {id}@{} does not match git revision ({})",
                        release.path.as_deref().unwrap_or("(root)"),
                        release.manifest.version,
                        path.as_deref().unwrap_or("(root)")
                    );
                }
                let checkout = txn.worktree(id, &relative, &next_commit, ssh)?;
                let m = package_manifest(&checkout, path.as_deref())?;
                ensure!(
                    m.id == id,
                    InvalidManifest,
                    "package id changed upstream to {}",
                    m.id
                );
                if let Some(release) = &expected {
                    verify_catalog_manifest(&m, release)?;
                }
                // Tags are not policed: an explicit tag or commit may keep the installed version
                // at another commit; when it is the moved release tag of that version, say so.
                if let Target::Ref(reference) = &target
                    && m.version == p.current.version
                    && *reference == identity::release_tag(&m.version)
                {
                    warnings.push(format!(
                        "release tag {reference} now points to {}",
                        &next_commit[..next_commit.len().min(12)]
                    ));
                }
                // A protected package takes new dependencies from its own catalog.
                let dependency_catalog = match &protected {
                    Some(location) => Some(location.clone()),
                    None => self.options.catalog.clone(),
                };
                self.check_change(txn, state, id, &m, dependency_catalog.as_deref())?;
                let next = revision(&checkout, &m, self.options.custom_ssh)?;
                let entry = state.packages.get_mut(id).unwrap();
                entry.previous = Some(p.current.with_mode(&p.mode));
                entry.previous_build = p.build.clone();
                entry.build = None;
                if transitioning_build {
                    entry.catalog = None;
                }
                entry.current = next.clone();
                entry.path = path.clone();
                entry.mode = mode.clone();
                entry.adopt(&m);
                // Only active packages satisfy the dependencies of an active package.
                if state.is_active(id) {
                    deps::refuse_inactive_dependencies(state, id, "update", false)?;
                }
                let staged = txn.deploy(
                    id,
                    &relative,
                    path.as_deref(),
                    &next.commit,
                    self.options.custom_ssh,
                )?;
                if let Some(release) = &expected {
                    verify_catalog_release(&m, release, &staged)?;
                }
                Ok(RevisionChange {
                    id: id.into(),
                    major: deps::is_major_text(&p.current.version, &next.version),
                    from: p.current.clone(),
                    to: next,
                    changed: true,
                    warnings: warnings.into_iter().chain(metadata_warnings(&m)).collect(),
                    skipped,
                    unchanged_reason: None,
                    mode,
                    detached,
                    dist: DistSync::default(),
                })
            })
            .map_err(|error| error.with_package(id).listing_lost_work())?;
        Ok(RevisionChange { dist, ..change })
    }
    /// Swap current and previous version (offline). Refused if it breaks a dependent.
    pub fn rollback(&self, id: &str) -> Result<RevisionChange> {
        valid_id(id)?;
        let ssh = self.options.custom_ssh;
        let (change, dist) = self
            .transaction(|txn, state| {
                let p = state
                    .packages
                    .get(id)
                    .cloned()
                    .ok_or_else(|| Error::NotInstalled { id: id.into() })?;
                let previous = p.previous.clone().ok_or_else(|| Error::NoPreviousVersion {
                    id: id.into(),
                    commit: None,
                })?;
                if let Some(build) = &p.previous_build {
                    let cached = txn.root().join(crate::artifact::relative(id, build));
                    if !cached.is_dir() {
                        return Err(Error::NoPreviousVersion {
                            id: id.into(),
                            commit: Some(previous.commit.clone()),
                        });
                    }
                    if p.build.is_none() {
                        let live = installed_repo(txn.root(), id, &p)?;
                        guard_clone(
                            &live,
                            state,
                            id,
                            "local commits detected",
                            "unpublished work blocks rollback",
                            false,
                            ssh,
                        )?;
                    }
                    self.check_change(txn, state, id, &build.release.manifest, None)?;
                    if p.build.is_none() {
                        txn.remove_worktree(id, &p.clone_folder(id))?;
                    }
                    let entry = state.packages.get_mut(id).unwrap();
                    entry.previous = Some(p.current.with_mode(&p.mode));
                    entry.previous_build = p.build.clone();
                    entry.current = previous.without_mode();
                    entry.mode = previous.mode.clone();
                    entry.path = build.release.path.clone();
                    entry.build = Some(build.clone());
                    entry.adopt(&build.release.manifest);
                    if state.is_active(id) {
                        deps::refuse_inactive_dependencies(state, id, "roll back", false)?;
                        builds::deploy_build(txn, id, build)?;
                    }
                    return Ok(RevisionChange {
                        id: id.into(),
                        from: p.current,
                        to: previous.without_mode(),
                        changed: true,
                        major: false,
                        warnings: Vec::new(),
                        skipped: Vec::new(),
                        unchanged_reason: None,
                        mode: previous.mode.clone(),
                        detached: None,
                        dist: DistSync::default(),
                    });
                }
                let relative = p.clone_folder(id);
                if p.build.is_none() {
                    let worktree = installed_repo(txn.root(), id, &p)?;
                    guard_clone(
                        &worktree,
                        state,
                        id,
                        "local commits detected",
                        "local branches, tags or stash hold unpublished work; \
                         keep or remove them before rolling back",
                        false,
                        ssh,
                    )?;
                }
                let live = source_repo(txn.root(), id, &p)?;
                // The previous commit can be gone: pruned after the origin deleted or moved
                // the tag that held it.
                let previous_commit = format!("{}^{{commit}}", previous.commit);
                if transport::require_object(&live, &previous_commit, ssh).is_err() {
                    return Err(Error::NoPreviousVersion {
                        id: id.into(),
                        commit: Some(previous.commit.clone()),
                    });
                }
                let repo = txn.repo(&relative, &live)?;
                let path = match packages::find_at_with_ssh(&repo, &previous.commit, Some(id), ssh)
                {
                    Ok(found) => found.subdir().map(str::to_owned),
                    Err(Error::InvalidArgument(_)) => {
                        fail!(InvalidManifest, "rollback manifest mismatch")
                    }
                    Err(error) => return Err(error),
                };
                let checkout = txn.worktree(id, &relative, &previous.commit, ssh)?;
                let m = package_manifest(&checkout, path.as_deref())?;
                ensure!(
                    m.id == id && m.version == previous.version,
                    InvalidManifest,
                    "rollback manifest mismatch"
                );
                self.check_change(txn, state, id, &m, None)?;
                // Revision and mode swap together.
                let entry = state.packages.get_mut(id).unwrap();
                entry.previous = Some(p.current.with_mode(&p.mode));
                entry.previous_build = p.build.clone();
                entry.build = None;
                entry.current = previous.without_mode();
                entry.mode = previous.mode.clone();
                entry.path = path.clone();
                entry.adopt(&m);
                if state.is_active(id) {
                    deps::refuse_inactive_dependencies(state, id, "roll back", false)?;
                }
                txn.deploy(
                    id,
                    &relative,
                    path.as_deref(),
                    &previous.commit,
                    self.options.custom_ssh,
                )?;
                Ok(RevisionChange {
                    id: id.into(),
                    from: p.current,
                    to: previous.without_mode(),
                    changed: true,
                    major: false,
                    warnings: Vec::new(),
                    skipped: Vec::new(),
                    unchanged_reason: None,
                    mode: previous.mode.clone(),
                    detached: None,
                    dist: DistSync::default(),
                })
            })
            .map_err(|error| error.with_package(id).listing_lost_work())?;
        Ok(RevisionChange { dist, ..change })
    }
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<DistSync> {
        self.set_enabled_with(id, enabled, false)
    }
    /// Disabling removes `dist/<id>`; enabling deploys it again from the clone at the
    /// installed commit (unless the package is replaced). Disabling drops the
    /// replacements the package provides (their originals are deployed again when
    /// active). Disabling is refused with [`Error::HasDependents`] unless `force` when a
    /// consumer would break (see [`UninstallPlan::dependents`]), and with
    /// [`Error::DependencyConflict`] when a reactivated original has inactive
    /// dependencies. Enabling a disabled
    /// package whose dependencies are not all satisfied by active packages (missing,
    /// disabled, replaced away or of a non-matching version) is refused with
    /// [`Error::DependencyConflict`] unless `force`; dependencies are never enabled
    /// along. Returns the dist report.
    pub fn set_enabled_with(&self, id: &str, enabled: bool, force: bool) -> Result<DistSync> {
        valid_id(id)?;
        self.transaction(|_, state| {
            let Some(package) = state.packages.get(id) else {
                return Err(Error::NotInstalled { id: id.into() });
            };
            if enabled && !force && !package.enabled && !state.replacements.contains_key(id) {
                deps::refuse_inactive_dependencies(state, id, "enable", true)?;
            }
            if !enabled {
                if !force {
                    let after = without_provider(state, id, false);
                    guard_removal(state, &after, id, "disable", &[])?;
                }
                state.replacements.retain(|_, fork| fork != id);
            }
            state.packages.get_mut(id).unwrap().enabled = enabled;
            Ok(())
        })
        .map(|((), dist)| dist)
    }
    /// Enable `id`, refusing inactive dependencies (see [`Manager::set_enabled_with`]).
    pub fn enable(&self, id: &str) -> Result<DistSync> {
        self.enable_with(id, false)
    }
    /// Enable `id`; with `force` also when its dependencies are not active (it is then
    /// reported broken by `list` and `check`).
    pub fn enable_with(&self, id: &str, force: bool) -> Result<DistSync> {
        self.set_enabled_with(id, true, force)
    }
    pub fn disable(&self, id: &str) -> Result<DistSync> {
        self.set_enabled(id, false)
    }
    /// Forget registration but retain the repository; `dist/<id>` is removed (a
    /// forgotten package is not installed) and `restore` deploys it again.
    pub fn forget(&self, id: &str) -> Result<ForgetResult> {
        self.forget_with(id, false)
    }
    /// Like [`Manager::forget`]; consumers that would break refuse unless `force` (the
    /// guard of [`Manager::set_enabled_with`]). Replacements this package provides are
    /// dropped (their consumers go back to the original, whose `dist/` is deployed
    /// again). The whole record is kept in `State::forgotten` (source-keyed clone), so
    /// `restore` records it again as it was and `uninstall` finds it. Its worktree
    /// stays at its recorded commit, independently of siblings. A package
    /// whose clone is gone keeps no `forgotten` record: there is nothing to restore.
    pub fn forget_with(&self, id: &str, force: bool) -> Result<ForgetResult> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            if !state.packages.contains_key(id) {
                return Err(Error::NotInstalled { id: id.into() });
            }
            if !force {
                let after = without_provider(state, id, true);
                guard_removal(state, &after, id, "forget", &[])?;
            }
            let installed = state.packages.remove(id).unwrap();
            state.replacements.retain(|_, fork| fork != id);
            if installed.link().is_some() {
                state.forgotten.insert(id.to_owned(), installed);
                return Ok(false);
            }
            if installed.build.is_some() {
                state.forgotten.insert(id.to_owned(), installed);
                return Ok(true);
            }
            match installed_repo(txn.root(), id, &installed) {
                // Nothing to restore: only the record (and dist) are dropped.
                Err(Error::RepositoryNotFound { .. }) => {
                    let source = match clones::source_repo(txn.root(), id, &installed) {
                        Ok(bare) => Some(bare),
                        Err(Error::RepositoryNotFound { .. }) => None,
                        Err(error) => return Err(error),
                    };
                    if let Some(key) = &installed.clone
                        && let Some(bare) = source
                    {
                        let relative = installed.clone_folder(id);
                        txn.remove_worktree(id, &relative)?;
                        if clones::clone_users(state, key, id).is_empty() {
                            let hidden = lost_work::hidden(&bare, false, self.options.custom_ssh)?;
                            if !hidden.is_empty() {
                                return Err(lost_work::refusal(id, hidden));
                            }
                            txn.delete_repo(&relative)?;
                        }
                    }
                    txn.delete_dist(id)?;
                    return Ok(false);
                }
                Err(error) => return Err(error),
                Ok(_) => {}
            }
            state.forgotten.insert(id.to_owned(), installed);
            Ok(true)
        })
        .map(|(clone_kept, dist)| ForgetResult { clone_kept, dist })
    }
    /// Record a forgotten Git or build package again with its recorded revisions,
    /// catalog, enabled flag and mode. Git worktrees must be clean and at the
    /// recorded commit, including branch mode. Deploy when active.
    pub fn restore(&self, id: &str) -> Result<InstallResult> {
        valid_id(id)?;
        let (result, dist) = self.transaction(|txn, state| {
            if state.packages.contains_key(id) {
                return Err(Error::AlreadyInstalled {
                    id: id.into(),
                    message: format!("package {id} is already registered"),
                });
            }
            let record = state
                .forgotten
                .get(id)
                .cloned()
                .ok_or_else(|| Error::RepositoryNotFound { id: id.into() })?;
            self.restore_record(txn, state, id, record)
        })?;
        Ok(InstallResult { dist, ..result })
    }
    /// [`Manager::restore`]: record the forgotten package `id` again from `record` and
    /// deploy it when active.
    fn restore_record(
        &self,
        txn: &mut Txn,
        state: &mut State,
        id: &str,
        record: Installed,
    ) -> Result<InstallResult> {
        if record.build.is_some() || record.link().is_some() {
            let result = builds::build_result(id, &record, false);
            state.forgotten.remove(id);
            state.packages.insert(id.into(), record.clone());
            if state.is_active(id) {
                deps::refuse_inactive_dependencies(state, id, "restore", false)?;
                if record.link().is_none() {
                    deploy_committed(txn, id, &record, self.options.custom_ssh)?;
                }
            }
            return Ok(result);
        }
        let relative = record.clone_folder(id);
        // The clone is gone: `repository_not_found`, like a restore without a record.
        let live = installed_repo(txn.root(), id, &record)?;
        guard_clone(
            &live,
            state,
            id,
            "worktree HEAD moved away from the recorded revision",
            "unpublished work blocks restore",
            false,
            self.options.custom_ssh,
        )?;
        let recorded = record.current.commit.clone();
        let commit = recorded.clone();
        let repo = live;
        let path =
            match packages::find_at_with_ssh(&repo, &commit, Some(id), self.options.custom_ssh) {
                Ok(found) => found.subdir().map(str::to_owned),
                Err(Error::InvalidArgument(_)) => fail!(InvalidManifest, "package id mismatch"),
                Err(error) => return Err(error),
            };
        let m = package_manifest(&repo, path.as_deref())?;
        ensure!(
            m.id == id && (commit != recorded || m.version == record.current.version),
            InvalidManifest,
            "the recorded revision of {id} has another manifest"
        );
        let mut installed = Installed {
            path: path.clone(),
            ..record
        };
        if commit != recorded {
            installed.previous = Some(installed.current.with_mode(&installed.mode));
            installed.current = revision(&repo, &m, self.options.custom_ssh)?;
        }
        let result = InstallResult {
            id: id.into(),
            source: installed.source.clone(),
            current: installed.current.clone(),
            fork_of: m.fork_of.clone(),
            skipped: Vec::new(),
            warnings: metadata_warnings(&m),
            already_installed: false,
            available: None,
            mode: installed.mode.clone(),
            requested_mode: None,
            enabled: installed.enabled,
            dependencies: Vec::new(),
            dist: DistSync::default(),
        };
        let moved = commit != recorded;
        state.forgotten.remove(id);
        state.packages.insert(id.to_owned(), installed);
        if moved {
            // A newer branch commit must keep the workspace's dependencies working.
            self.check_change(txn, state, id, &m, None)?;
        }
        state.packages.get_mut(id).unwrap().adopt(&m);
        if state.is_active(id) {
            deps::refuse_inactive_dependencies(state, id, "restore", false)?;
            txn.deploy(
                id,
                &relative,
                path.as_deref(),
                &commit,
                self.options.custom_ssh,
            )?;
        }
        Ok(result)
    }
    /// Read-only: what `uninstall` would delete, which local work would be lost and
    /// which installed packages depend on it.
    /// Matches the exact ID spelling (never a case variant on a case-insensitive FS).
    pub fn uninstall_plan(&self, id: &str) -> Result<UninstallPlan> {
        valid_id(id)?;
        let _busy = self.busy();
        let state = self.state()?;
        let removal = removal(&self.root, &state, id, self.options.custom_ssh)?;
        let mut deletes = Vec::new();
        let checkout = self.root.join("worktrees").join(id);
        if removal.key.is_some() && checkout.is_dir() {
            deletes.push(checkout.to_string_lossy().into_owned());
        }
        if let Some(repo) = &removal.repo {
            deletes.push(repo.to_string_lossy().into_owned());
        }
        let cache = self.root.join("builds").join(id);
        if cache.is_dir() {
            deletes.push(cache.to_string_lossy().into_owned());
        }
        let dist = self.root.join("dist").join(id);
        if fs::symlink_metadata(&dist).is_ok() {
            deletes.push(dist.to_string_lossy().into_owned());
        }
        let after = without_provider(&state, id, true);
        Ok(UninstallPlan {
            id: id.into(),
            registered: state.packages.contains_key(id),
            deletes,
            dependents: deps::breaking_dependents(&state, id, &after),
            dependency_conflict: reactivation_conflict(&state, &after, id, "uninstall")
                .map(|error| error.to_string()),
            lost_work: removal.lost_work,
        })
    }
    /// Remove state entry, repository and dist; replacements this package provides are
    /// dropped (the originals become active again and their `dist/` is deployed). For a
    /// package of a shared clone the clone
    /// stays while other packages (registered or forgotten) use it. Each package's
    /// detached worktree is removed independently. A forgotten package is uninstalled the same way (its `dist/`, its
    /// record and the clone if it was the last user); so is a recorded package whose
    /// clone is gone (only its record and `dist/` remain, nothing is lost). Local work
    /// (changes, untracked/ignored files, unpublished commits/branches/tags, stash) not
    /// in `confirmed` (the [`UninstallPlan::lost_work`] the user saw) refuses with
    /// [`Error::LocalChanges`] listing the unconfirmed items, so work that appeared
    /// after the plan is never deleted unseen. Consumers that would break refuse with
    /// [`Error::HasDependents`] (carrying the lost work, see [`UninstallPlan::refusal`])
    /// unless `force`; they then report an unsatisfied dependency.
    pub fn uninstall(&self, id: &str, confirmed: &[LostWork], force: bool) -> Result<DistSync> {
        valid_id(id)?;
        self.transaction(|txn, state| {
            let removal = removal(txn.root(), state, id, self.options.custom_ssh)?;
            if !force {
                let after = without_provider(state, id, true);
                guard_removal(state, &after, id, "uninstall", &removal.lost_work)?;
            }
            let unconfirmed: Vec<LostWork> = removal
                .lost_work
                .iter()
                .filter(|item| !confirmed.contains(item))
                .cloned()
                .collect();
            if !unconfirmed.is_empty() {
                return Err(lost_work::refusal(id, unconfirmed));
            }
            state.packages.remove(id);
            state.forgotten.remove(id);
            state.replacements.retain(|_, fork| fork != id);
            if removal.key.is_some() {
                txn.remove_worktree(id, &removal.relative)?;
            }
            if removal.repo.is_some() {
                txn.delete_repo(&removal.relative)?;
            }
            txn.delete_dist(id)?;
            Ok(())
        })
        .map(|((), dist)| dist)
    }

    /// Read-only replacement metadata plan. Use `replace_check` for workspace diagnostics.
    pub fn replace_plan(&self, original: &str, replacement: &str) -> Result<ReplacementPlan> {
        let _busy = self.busy();
        replacement_plan(&self.state()?, original, replacement)
    }

    /// Check the entire proposed active workspace without changing files or state.
    /// Installed versions stay fixed; missing releases are selected from the catalog.
    pub fn replace_check(&self, original: &str, replacement: &str) -> Result<ReplacementCheck> {
        let _busy = self.busy();
        replacement_check(
            &self.state()?,
            original,
            replacement,
            self.options.catalog.as_deref(),
        )
    }

    /// Explicitly install missing dependencies for a proposed replacement, without
    /// applying its binding, enabling installed packages or updating installed versions.
    pub fn replace_resolve(
        &self,
        original: &str,
        replacement: &str,
    ) -> Result<ReplacementResolution> {
        let (mut resolution, dist) = self.transaction(|txn, state| {
            let check = replacement_check(
                state,
                original,
                replacement,
                self.options.catalog.as_deref(),
            )?;
            check.refuse_unresolvable(state)?;
            let missing = check
                .missing
                .iter()
                .map(|(id, v)| Ok((id.clone(), deps::version(v)?)))
                .collect::<Result<BTreeMap<_, _>>>()?;
            let installed =
                self.install_missing(txn, state, &missing, true, self.options.catalog.as_deref())?;
            let check = replacement_check(
                state,
                original,
                replacement,
                self.options.catalog.as_deref(),
            )?;
            check.refuse(state)?;
            Ok(ReplacementResolution {
                installed,
                check,
                dist: DistSync::default(),
            })
        })?;
        resolution.dist = dist;
        Ok(resolution)
    }

    /// Apply a consistent replacement atomically. Passing the original as replacement
    /// removes its binding. No dependencies are installed and there is no force bypass.
    pub fn replace(&self, original: &str, replacement: &str) -> Result<ReplacementPlan> {
        let (plan, dist) = self.transaction(|_, state| {
            let check = replacement_check(
                state,
                original,
                replacement,
                self.options.catalog.as_deref(),
            )?;
            check.refuse(state)?;
            apply_replacement(state, original, replacement);
            Ok(check.plan)
        })?;
        Ok(ReplacementPlan { dist, ..plan })
    }

    /// Read-only: what [`Manager::sync_dist`] would change.
    pub fn sync_dist_plan(&self) -> Result<DistSync> {
        let _busy = self.busy();
        dist_drift(&self.root, &self.state()?, self.options.custom_ssh)
    }
    /// Make `dist/` hold exactly the active packages: deploy a missing `dist/<id>` of an
    /// active package from its clone at the installed commit and remove the `dist/` of
    /// inactive ones (for example a folder copied into `dist/` by hand). An existing `dist/<id>` of an active package is left as it is;
    /// unknown folders are only reported. Every mutating operation does the same at its
    /// end; this one changes nothing else. A missing `dist/` that cannot be deployed
    /// (its clone is gone) is reported as an error here.
    /// A package that cannot be deployed (see [`DistSync::skipped`], or any other error
    /// for it) is reported in `skipped` and the others are still synced: the transaction
    /// commits, then the call fails with [`Error::NotDeployed`] carrying the report.
    pub fn sync_dist(&self) -> Result<DistSync> {
        let (mut plan, report) = self.transaction(|txn, state| {
            let mut plan = dist_drift(txn.root(), state, self.options.custom_ssh)?;
            let mut deployed = Vec::new();
            for id in std::mem::take(&mut plan.deployed) {
                match deploy_committed(txn, &id, &state.packages[&id], self.options.custom_ssh) {
                    Ok(()) => deployed.push(id),
                    Err(error) => {
                        // Settled as "nothing to deploy" (the live dist is missing).
                        txn.delete_dist(&id)?;
                        plan.skipped.push(SkippedDist {
                            id: id.clone(),
                            clone_missing: matches!(error, Error::RepositoryNotFound { .. }),
                            link: state.packages[&id].link().cloned(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
            plan.deployed = deployed;
            plan.skipped.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(plan)
        })?;
        plan.warnings = report.warnings;
        if plan.skipped.is_empty() {
            return Ok(plan);
        }
        let ids: Vec<&str> = plan.skipped.iter().map(|s| s.id.as_str()).collect();
        Err(Error::NotDeployed {
            message: format!("{} package(s) not deployed: {}", ids.len(), ids.join(", ")),
            sync: Box::new(plan),
        })
    }
}

/// Bring all of `dist/` in line with [`State::is_active`] at the end of every mutating
/// operation, inside its transaction: a staged or live `dist/<id>` of an inactive known
/// package is removed; an active package whose `dist/` is missing, or that just became
/// active, gets it from its clone at the installed commit. Folders of no known package
/// are left alone, and so is an active package that cannot be deployed because its clone
/// or installed commit is missing or its namespace path is not a folder (callers report
/// these with [`Manager::sync_dist_plan`]); any other failure fails the operation. Cheap:
/// one listing of `dist/`, and Git reads only for the packages deployed.
fn settle_dist(txn: &mut Txn, before: &State, after: &State, custom_ssh: bool) -> Result<()> {
    for (id, old) in before.packages.iter().chain(&before.forgotten) {
        let next = after.packages.get(id).or_else(|| after.forgotten.get(id));
        for build in old.build.iter().chain(&old.previous_build) {
            let relative = crate::artifact::relative(id, build);
            let keep = next.is_some_and(|p| {
                p.build
                    .iter()
                    .chain(&p.previous_build)
                    .any(|b| crate::artifact::relative(id, b) == relative)
            });
            if !keep {
                txn.delete_repo(&relative)?;
            }
        }
    }
    let staged = txn.staged_dists().into_iter().collect();
    let drift = dist_drift_unstaged(txn.root(), after, custom_ssh, &staged)?;
    for id in txn.staged_dists().into_iter().chain(drift.removed) {
        if !after.is_active(&id) {
            txn.delete_dist(&id)?;
        }
    }
    for (id, p) in &after.packages {
        if !after.is_active(id) || txn.has_dist(id) || drift.skipped.iter().any(|s| s.id == *id) {
            continue;
        }
        if drift.deployed.contains(id) || !before.is_active(id) {
            deploy_committed(txn, id, p, custom_ssh)?;
        }
    }
    Ok(())
}

/// Stage `dist/<id>` from the package's `dist/` at its installed commit (see
/// [`Txn::deploy`]); the clone must exist.
fn deploy_committed(txn: &mut Txn, id: &str, p: &Installed, custom_ssh: bool) -> Result<()> {
    if let Some(link) = p.link() {
        ensure!(
            Path::new(&link.target).is_dir(),
            Workspace,
            "linked output unavailable: {}",
            link.target
        );
        return txn.deploy_link(id, Path::new(&link.target));
    }
    if let Some(build) = &p.build {
        return builds::deploy_build(txn, id, build);
    }
    let relative = p.clone_folder(id);
    if txn.staged_repo(&relative).is_none() {
        installed_repo(txn.root(), id, p)?;
    }
    txn.deploy(
        id,
        &relative,
        p.path.as_deref(),
        &p.current.commit,
        custom_ssh,
    )?;
    Ok(())
}

/// Compare `dist/` with the active packages of `state` (read-only).
fn dist_drift(root: &Path, state: &State, custom_ssh: bool) -> Result<DistSync> {
    dist_drift_unstaged(root, state, custom_ssh, &BTreeSet::new())
}
fn dist_drift_unstaged(
    root: &Path,
    state: &State,
    custom_ssh: bool,
    staged: &BTreeSet<String>,
) -> Result<DistSync> {
    let mut plan = DistSync::default();
    let dist = root.join("dist");
    // Known IDs come from installed and forgotten records.
    let known = |id: &str| {
        valid_id(id).is_ok()
            && (state.packages.contains_key(id) || state.forgotten.contains_key(id))
    };
    let is_namespace = |name: &str| {
        state
            .packages
            .keys()
            .chain(state.forgotten.keys())
            .any(|id| id.split_once('/').is_some_and(|(ns, _)| ns == name))
    };
    // (name, real directory: not a symlink, not a file)
    let names = |folder: &Path| -> Result<Vec<(String, bool)>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            let directory = entry.file_type()?.is_dir();
            names.push((entry.file_name().to_string_lossy().into_owned(), directory));
        }
        names.sort();
        Ok(names)
    };
    let mut present = Vec::new();
    // Namespace paths that are not real folders: nothing is deployed below them.
    let mut blocked = Vec::new();
    if dist.is_dir() {
        for (name, directory) in names(&dist)? {
            if known(&name) {
                present.push(name);
            } else if is_namespace(&name) && !directory {
                plan.unmanaged.push(format!("dist/{name}"));
                blocked.push(name);
            } else if directory && is_namespace(&name) {
                for (child, _) in names(&dist.join(&name))? {
                    let id = format!("{name}/{child}");
                    if known(&id) {
                        present.push(id);
                    } else {
                        plan.unmanaged.push(format!("dist/{id}"));
                    }
                }
            } else {
                plan.unmanaged.push(format!("dist/{name}"));
            }
        }
    }
    plan.removed = present
        .iter()
        .filter(|id| !state.is_active(id))
        .cloned()
        .collect();
    for (id, p) in &state.packages {
        if !state.is_active(id) {
            continue;
        }
        // A blocked namespace is unmanaged, and its active packages remain undeployed.
        let namespace = id.split_once('/').map(|(ns, _)| ns);
        if let Some(ns) = namespace
            && blocked.iter().any(|b| b == ns)
        {
            plan.skipped.push(SkippedDist {
                id: id.clone(),
                reason: format!("dist/{ns} is not a folder"),
                clone_missing: false,
                link: p.link().cloned(),
            });
            continue;
        }
        if let Some(link) = p.link() {
            if !state.is_active(id) || staged.contains(id) {
                continue;
            }
            let leaf = root.join("dist").join(id);
            if let Ok(meta) = fs::symlink_metadata(&leaf)
                && (!meta.file_type().is_symlink()
                    || fs::read_link(&leaf)? != Path::new(&link.target))
            {
                plan.skipped.push(SkippedDist {
                    id: id.clone(),
                    reason: format!("unexpected linked deployment entry: {}", leaf.display()),
                    clone_missing: false,
                    link: Some(link.clone()),
                });
                continue;
            }
            if !Path::new(&link.target).is_dir() {
                plan.skipped.push(SkippedDist {
                    id: id.clone(),
                    reason: format!("dev-linked output unavailable: {}", link.target),
                    clone_missing: false,
                    link: Some(link.clone()),
                });
                continue;
            }
            if !present.contains(id) {
                plan.deployed.push(id.clone());
            }
            continue;
        }
        if !state.is_active(id) || present.contains(id) {
            continue;
        }
        if let Some(build) = &p.build {
            if root.join(crate::artifact::relative(id, build)).is_dir() {
                plan.deployed.push(id.clone());
            } else {
                plan.skipped.push(SkippedDist {
                    id: id.clone(),
                    reason: "cached build missing".into(),
                    clone_missing: false,
                    link: None,
                });
            }
            continue;
        }
        let commit = format!("{}^{{commit}}", p.current.commit);
        let reason = match installed_repo(root, id, p) {
            Err(Error::RepositoryNotFound { .. }) => Some(("clone missing", true)),
            Err(error) => return Err(error),
            Ok(repo) => transport::require_object(&repo, &commit, custom_ssh)
                .is_err()
                .then_some(("installed commit missing from its clone", false)),
        };
        match reason {
            Some((reason, clone_missing)) => plan.skipped.push(SkippedDist {
                id: id.clone(),
                reason: reason.to_owned(),
                clone_missing,
                link: None,
            }),
            None => plan.deployed.push(id.clone()),
        }
    }
    plan.removed.sort();
    Ok(plan)
}

/// Which revision of the source an install takes.
#[derive(Clone, Copy)]
enum FetchTarget<'a> {
    /// The newest release tag (release mode).
    Latest,
    /// Exactly this commit of a catalog release (release mode).
    Exact(&'a str),
    /// A user ref resolved against the origin (pinned or branch mode).
    Ref(&'a str),
    /// The origin's default branch (branch mode).
    Dev,
}
/// Where an install takes its revision from.
#[derive(Clone, Copy)]
struct Fetch<'a> {
    source: &'a str,
    target: FetchTarget<'a>,
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
            target: FetchTarget::Exact(&release.commit),
            expected: Some(&release.manifest),
            files: (!release.files.is_empty()).then_some(&release.files),
            package: Some(&release.manifest.id),
            path: release.path.as_deref(),
        }
    }
}

/// A scratch bare source with a verified installation revision.
struct Cloned {
    repo: PathBuf,
    commit: String,
    manifest: Manifest,
    /// Package folder (`None`: the repository root).
    path: Option<String>,
    expected_files: Option<BTreeMap<String, catalog::FileDigest>>,
    skipped: Vec<releases::SkippedTag>,
    /// The mode the install records.
    mode: Mode,
}

/// Bare-clone into scratch and verify the selected revision from its Git objects.
fn clone_revision(txn: &Txn, fetch: &Fetch, name: &str, custom_ssh: bool) -> Result<Cloned> {
    let repo = txn.scratch(name);
    let parent = repo.parent().unwrap();
    fs::create_dir_all(parent)?;
    crate::transport::clone_bare(parent, fetch.source, name, custom_ssh)?;
    // A fresh clone has exactly the origin's tags.
    lost_work::record_origin_tags(&repo, custom_ssh)?;
    let mut skipped = Vec::new();
    let (commit, mode) = match fetch.target {
        FetchTarget::Exact(commit) => (resolve_ref(&repo, commit, custom_ssh)?, Mode::Release),
        // Source branches exist only under refs/remotes/origin; package checkouts
        // are detached worktrees created when adopting the verified revision.
        FetchTarget::Ref(reference) => resolve_upstream_ref(&repo, reference, custom_ssh)?,
        FetchTarget::Dev => resolve_upstream_ref(&repo, "HEAD", custom_ssh)?,
        FetchTarget::Latest => {
            let mut found = match releases::discover_all_with_ssh(&repo, fetch.package, custom_ssh)
            {
                // An ID the repository does not have: say which packages it has.
                Err(Error::NoReleases(message)) => {
                    if let Some(id) = fetch.package {
                        let head = transport::object_id(&repo, "HEAD", custom_ssh)?;
                        packages::find_at_with_ssh(&repo, &head, Some(id), custom_ssh)?;
                    }
                    return Err(Error::NoReleases(message));
                }
                other => other?,
            };
            // Pre-releases are installed only by name.
            let id = found
                .tags
                .first()
                .map(|tag| tag.manifest.id.clone())
                .or_else(|| fetch.package.map(str::to_owned))
                .unwrap_or_default();
            if found.offered(None) && found.tags.is_empty() {
                if found.skipped.is_empty() {
                    fail!(
                        NoReleases,
                        "{id} has only pre-release tags; install one by ref (vVERSION)"
                    );
                }
                fail!(
                    NoReleases,
                    "no usable release tags for {id} (pre-release tags are installed only by ref){}",
                    found
                        .skipped
                        .iter()
                        .map(|s| format!("; skipped {}", s.reason))
                        .collect::<String>()
                );
            }
            found.ensure_latest_is_valid()?;
            skipped = std::mem::take(&mut found.skipped);
            let latest = found.tags.pop().require(Error::NoReleases, "no releases")?;
            (latest.commit, Mode::Release)
        }
    };
    let package = fetch.package;
    let found = packages::find_at_with_ssh(&repo, &commit, package, custom_ssh)?;
    if fetch.expected.is_some() {
        ensure!(
            found.subdir() == fetch.path,
            Catalog,
            "catalog package folder {} does not match git revision ({})",
            fetch.path.unwrap_or("(root)"),
            found.subdir().unwrap_or("(root)")
        );
    }
    let path = found.subdir().map(str::to_owned);
    let m = found.manifest;
    if let Some(expected) = fetch.expected {
        ensure!(
            serde_json::to_value(&m)? == serde_json::to_value(expected)?,
            Catalog,
            "catalog manifest does not match git revision"
        );
    }
    if let Some(expected_files) = fetch.files {
        ensure!(
            catalog::hash_exported_dist(
                &repo,
                &commit,
                path.as_deref(),
                &txn.scratch("verify-dist"),
                custom_ssh,
            )?
            .files
                == *expected_files,
            Catalog,
            "catalog file hashes do not match git dist"
        );
    }
    Ok(Cloned {
        repo,
        commit,
        manifest: m,
        path,
        expected_files: fetch.files.cloned(),
        skipped,
        mode,
    })
}

/// The new package's ID must not collide with anything installed or retained.
fn check_new_id(root: &Path, state: &State, m: &Manifest) -> Result<()> {
    if state.packages.contains_key(&m.id) {
        return Err(Error::AlreadyInstalled {
            message: format!("already installed: {}", m.id),
            id: m.id.clone(),
        });
    }
    if state.forgotten.contains_key(&m.id) {
        return Err(Error::AlreadyInstalled {
            message: format!("{} was forgotten and its clone is kept", m.id),
            id: m.id.clone(),
        });
    }
    let taken = || state.packages.keys().chain(state.forgotten.keys());
    if let Some(other) = taken().find(|id| ids_collide(id, &m.id)) {
        return Err(Error::Workspace(format!(
            "package ID {} differs only in letter case from installed {other}",
            m.id
        )));
    }
    ensure_no_case_collision(&root.join("dist"), &m.id)?;
    Ok(())
}

/// Stage the verified clone as an installed package and register it; a disabled one
/// (download) is not deployed.
fn adopt(
    txn: &mut Txn,
    state: &mut State,
    cloned: Cloned,
    source: String,
    catalog: Option<String>,
    enabled: bool,
    custom_ssh: bool,
) -> Result<InstallResult> {
    let (staged, relative, key) = place_clone(txn, state, &cloned, &source, custom_ssh)?;
    let m = &cloned.manifest;
    let current = revision(&staged, m, custom_ssh)?;
    if enabled {
        let staged = txn.deploy(
            &m.id,
            &relative,
            cloned.path.as_deref(),
            &current.commit,
            custom_ssh,
        )?;
        if let Some(expected) = &cloned.expected_files {
            ensure!(
                catalog::hash_files(&staged)? == *expected,
                Catalog,
                "catalog file hashes do not match git dist"
            );
        }
    }
    let mut installed = Installed {
        source: source.clone(),
        current: current.clone(),
        previous: None,
        enabled,
        mode: cloned.mode.clone(),
        catalog,
        clone: key,
        path: cloned.path.clone(),
        ..Default::default()
    };
    installed.adopt(m);
    state.packages.insert(m.id.clone(), installed);
    Ok(InstallResult {
        id: m.id.clone(),
        source,
        current,
        fork_of: m.fork_of.clone(),
        skipped: Vec::new(),
        warnings: metadata_warnings(m),
        already_installed: false,
        available: None,
        mode: cloned.mode.clone(),
        requested_mode: None,
        enabled,
        dependencies: Vec::new(),
        dist: DistSync::default(),
    })
}

/// Install `release` of dependency `id` that an update pulls; its refusal names the
/// dependency, not the package that pulled it.
#[expect(
    clippy::too_many_arguments,
    reason = "pulled dependency inputs include the caller SSH policy"
)]
fn install_pulled(
    txn: &mut Txn,
    state: &mut State,
    id: &str,
    release: &catalog::Release,
    name: &str,
    catalog: Option<String>,
    enabled: bool,
    custom_ssh: bool,
) -> Result<InstallResult> {
    (|| {
        let cloned = clone_revision(txn, &Fetch::release(release), name, custom_ssh)?;
        check_new_id(txn.root(), state, &cloned.manifest)?;
        adopt(
            txn,
            state,
            cloned,
            release.source.clone(),
            catalog,
            enabled,
            custom_ssh,
        )
    })()
    .map_err(|error| error.with_package(id))
}
/// [`Manager::protecting_catalog`] for the catalog at `location`.
fn protecting_catalog_at(location: &str, state: &State) -> Result<Option<String>> {
    if !catalog::is_verified(location)? {
        return Ok(None);
    }
    let origin = catalog::canonical_location(location)?;
    ensure_single_catalog(state, &origin)?;
    Ok(Some(origin))
}
/// The releases of protected `id` its verified catalog at `location` lists above the
/// installed version (ascending), and the newest origin release (the tags fetched into
/// `repo`) above every catalog version and the installed one, which the catalog lacks.
/// Refused with `catalog` when the catalog is no longer verified or lists the package
/// from another source.
fn catalog_candidates(
    repo: &Path,
    id: &str,
    installed: &Installed,
    location: &str,
    custom_ssh: bool,
) -> Result<(Vec<catalog::Release>, Option<String>)> {
    ensure!(
        catalog::is_verified(location)?,
        Catalog,
        "catalog {location} that protects {id} is no longer verified; detach {id} to update it from its origin"
    );
    let current = catalog::version_key(&installed.current.version)?;
    // Pre-releases only on the installed pre-release's track (opt-in).
    let mut listed = catalog::versions(location, id)?
        .into_iter()
        .map(|version| Ok((catalog::version_key(&version)?, version)))
        .filter(|found| {
            found
                .as_ref()
                .map_or(true, |(key, _)| deps::follows(Some(&current), key))
        })
        .collect::<Result<Vec<_>>>()?;
    listed.sort();
    let top = listed
        .last()
        .map(|(key, _)| key.clone())
        .unwrap_or_else(|| current.clone())
        .max(current.clone());
    let mut releases = Vec::new();
    for (key, version) in listed {
        if key <= current {
            continue;
        }
        let release = catalog::lookup(location, id, Some(&version))?;
        ensure!(
            release.source == installed.source,
            Catalog,
            "catalog {location} lists {id} from {}, not from its recorded source {}",
            release.source,
            installed.source
        );
        releases.push(release);
    }
    // Tag names only: the origin's newer release is reported, never installed.
    let upstream = match releases::discover_all_with_ssh(repo, Some(id), custom_ssh) {
        Ok(found) => found
            .tags
            .iter()
            .filter_map(|tag| {
                Some((
                    catalog::version_key(&tag.manifest.version).ok()?,
                    &tag.manifest.version,
                ))
            })
            .filter(|(key, _)| *key > top && deps::follows(Some(&current), key))
            .max()
            .map(|(_, version)| version.clone()),
        Err(_) => None,
    };
    Ok((releases, upstream))
}
/// The installed revision of `id` must be the release record of its version in the
/// verified catalog at `location`: the same source, commit, package folder,
/// manifest and exported file hashes in the source object store.
fn verify_installed_release(
    txn: &Txn,
    repo: &Path,
    id: &str,
    installed: &Installed,
    location: &str,
    custom_ssh: bool,
) -> Result<()> {
    let version = &installed.current.version;
    let release = catalog::lookup(location, id, Some(version))?;
    ensure!(
        release.source == installed.source && release.commit == installed.current.commit,
        Catalog,
        "installed {id}@{version} at {} is not the release the catalog lists ({}); it cannot be protected by that catalog",
        &installed.current.commit[..installed.current.commit.len().min(12)],
        &release.commit[..release.commit.len().min(12)]
    );
    ensure_release_commit(repo, &release, custom_ssh)?;
    let found = packages::find_at_with_ssh(repo, &release.commit, Some(id), custom_ssh)?;
    ensure!(
        found.subdir() == release.path.as_deref(),
        Catalog,
        "catalog package folder of {id}@{version} does not match git revision"
    );
    let actual = found.manifest;
    let staged = txn.scratch("verify-installed-dist");
    if staged.exists() {
        fs::remove_dir_all(&staged)?;
    }
    export_dist(
        repo,
        &release.commit,
        release.path.as_deref(),
        &staged,
        custom_ssh,
    )?;
    verify_catalog_release(&actual, &release, &staged)
}
/// The commit of a catalog release must be in `repo` (the clone after fetching its origin).
fn ensure_release_commit(repo: &Path, release: &catalog::Release, custom_ssh: bool) -> Result<()> {
    ensure!(
        transport::require_object(repo, &format!("{}^{{commit}}", release.commit), custom_ssh)
            .is_ok(),
        Catalog,
        "commit {} of catalog release {}@{} is not in the package's clone after fetching its origin",
        release.commit,
        release.manifest.id,
        release.manifest.version
    );
    Ok(())
}
/// The checked-out manifest must match the catalog release.
fn verify_catalog_manifest(actual: &Manifest, release: &catalog::Release) -> Result<()> {
    let name = format!("{}@{}", release.manifest.id, release.manifest.version);
    ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(&release.manifest)?,
        Catalog,
        "catalog manifest does not match git revision of {name}: the release differs from the one added to the catalog"
    );
    Ok(())
}

/// Verify catalog hashes against the exported tree staged for deployment.
fn verify_catalog_release(
    actual: &Manifest,
    release: &catalog::Release,
    staged: &Path,
) -> Result<()> {
    verify_catalog_manifest(actual, release)?;
    let name = format!("{}@{}", release.manifest.id, release.manifest.version);
    ensure!(
        catalog::hash_files(staged)? == release.files,
        Catalog,
        "catalog file hashes do not match git dist of {name}: the release differs from the one added to the catalog"
    );
    Ok(())
}
/// One verified catalog per workspace: packages protected by another one must not be
/// mixed in.
fn ensure_single_catalog(state: &State, origin: &str) -> Result<()> {
    if let Some((id, other)) = state.packages.iter().find_map(|(id, p)| {
        p.catalog
            .as_deref()
            .filter(|c| *c != origin)
            .map(|c| (id, c))
    }) {
        return Err(Error::Catalog(format!(
            "this workspace uses verified catalog {other} ({id} was installed from it); \
             mixing verified catalogs is not supported, select that catalog, or detach or uninstall its packages"
        )));
    }
    Ok(())
}

/// The state once `id` provides nothing any more: disabled (`remove == false`) or
/// removed from `packages`, with the replacement bindings it provides dropped.
fn without_provider(state: &State, id: &str, remove: bool) -> State {
    let mut after = state.clone();
    if remove {
        after.packages.remove(id);
    } else if let Some(package) = after.packages.get_mut(id) {
        package.enabled = false;
    }
    after.replacements.retain(|_, fork| fork != id);
    after
}

/// The `has_dependents` refusal of `action` on `id` (one message for the guard and
/// [`UninstallPlan::refusal`]).
fn has_dependents(
    id: &str,
    action: &str,
    dependents: Vec<String>,
    lost_work: Vec<LostWork>,
) -> Error {
    Error::HasDependents {
        message: format!(
            "cannot {action} {id}: required by {}; doing it anyway requires force{}",
            dependents.join(", "),
            if lost_work.is_empty() {
                ""
            } else {
                "; the clone also has local work that would be lost"
            }
        ),
        id: id.into(),
        dependents,
        lost_work,
    }
}

/// Single guard of disable, forget and uninstall (unforced): consumers that break once
/// `id` stops providing (`after`, see [`deps::breaking_dependents`]) refuse with
/// [`Error::HasDependents`] carrying `lost_work`; an original that becomes active again
/// because a binding `id` provided is dropped needs its own direct dependencies active
/// ([`deps::refuse_inactive_dependencies`], as `enable`).
fn guard_removal(
    before: &State,
    after: &State,
    id: &str,
    action: &str,
    lost_work: &[LostWork],
) -> Result<()> {
    let dependents = deps::breaking_dependents(before, id, after);
    if !dependents.is_empty() {
        return Err(has_dependents(id, action, dependents, lost_work.to_vec()));
    }
    match reactivation_conflict(before, after, id, action) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// The [`Error::DependencyConflict`] of an original that becomes active again because
/// a binding `id` provided is dropped while its own direct dependencies are not active.
fn reactivation_conflict(before: &State, after: &State, id: &str, action: &str) -> Option<Error> {
    before
        .replacements
        .iter()
        .filter(|(original, fork)| *fork == id && after.is_active(original))
        .find_map(|(original, _)| {
            deps::refuse_inactive_dependencies(
                after,
                original,
                &format!("{action} {id}, which would reactivate"),
                true,
            )
            .err()
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
    /// Package that provides `original` afterwards (the original itself when switching back).
    pub provider: String,
    /// Version consumers see: the fork's declared `replaces.version`, or the original's
    /// when returning to it.
    pub version: String,
    /// Current provider if `original` is replaced now.
    pub current: Option<String>,
    /// Installed packages that depend on `original`.
    pub consumers: Vec<String>,
    /// Active consumers whose requirement `version` does not satisfy.
    pub incompatible: Vec<IncompatibleConsumer>,
    /// Installed version of `provider` (the fork; the original when switching back),
    /// which can differ from `version`, the one consumers see.
    pub installed_version: Option<String>,
    /// Reconciliation report of `replace` (empty for a plan).
    pub dist: DistSync,
}
/// One unsatisfied dependency in the proposed active workspace.
#[derive(Clone, Debug, Serialize)]
pub struct ReplacementIssue {
    pub consumer: String,
    pub dependency: deps::DependencyStatus,
}

/// A read-only workspace report. `issues` includes unrelated existing breakage.
/// `missing` contains a catalog solution for the proposed provider dependency closure
/// (including transitives), or is empty when `resolution_error` explains why none exists.
/// Only replacement constraints, including consumer requirements on the original, block apply.
#[derive(Clone, Debug, Serialize)]
pub struct ReplacementCheck {
    pub plan: ReplacementPlan,
    pub issues: Vec<ReplacementIssue>,
    pub missing: BTreeMap<String, String>,
    pub resolution_error: Option<String>,
}

/// Explicit dependency installs; the proposed binding has not been applied.
#[derive(Clone, Debug, Serialize)]
pub struct ReplacementResolution {
    pub installed: Vec<InstalledDependency>,
    pub check: ReplacementCheck,
    pub dist: DistSync,
}

impl ReplacementCheck {
    fn refuse_unresolvable(&self, state: &State) -> Result<()> {
        self.refuse_incompatible()?;
        if self.resolution_error.is_some() {
            return Err(self.conflict(state));
        }
        Ok(())
    }
    fn refuse(&self, state: &State) -> Result<()> {
        self.refuse_incompatible()?;
        if !self.missing.is_empty() || self.resolution_error.is_some() {
            return Err(self.conflict(state));
        }
        Ok(())
    }
    fn refuse_incompatible(&self) -> Result<()> {
        if self.plan.incompatible.is_empty() {
            return Ok(());
        }
        Err(Error::IncompatibleReplacement {
            message: format!(
                "{} provides {} {}, which some consumers do not accept: {}",
                self.plan.provider,
                self.plan.original,
                self.plan.version,
                self.plan
                    .incompatible
                    .iter()
                    .map(|c| format!("{} needs {} {}", c.id, self.plan.original, c.requirement))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            id: self.plan.original.clone(),
            consumers: self
                .plan
                .incompatible
                .iter()
                .map(|c| c.id.clone())
                .collect(),
        })
    }
    fn conflict(&self, state: &State) -> Error {
        Error::DependencyConflict(self.resolution_error.clone().unwrap_or_else(|| {
            let mut next = state.clone();
            apply_replacement(&mut next, &self.plan.original, &self.plan.provider);
            let mut pending = vec![self.plan.provider.clone()];
            let mut visited = BTreeSet::new();
            let mut needs = Vec::new();
            while let Some(id) = pending.pop() {
                if !visited.insert(id.clone()) {
                    continue;
                }
                for dependency in deps::status(&next, &next.packages[&id]) {
                    if let Some(provider) = dependency.provider {
                        pending.push(provider);
                    } else if self.missing.contains_key(&dependency.id) {
                        needs.push(format!(
                            "{id} needs {} {} (not installed)",
                            dependency.id, dependency.requirement
                        ));
                    }
                }
            }
            let missing = self
                .missing
                .iter()
                .map(|(id, version)| format!("{id} {version}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}; replacement installs nothing; missing: {missing}",
                needs.join("; ")
            )
        }))
    }
}

fn apply_replacement(state: &mut State, original: &str, replacement: &str) {
    if original == replacement {
        state.replacements.remove(original);
    } else {
        state
            .replacements
            .insert(original.into(), replacement.into());
    }
}

fn replacement_check(
    state: &State,
    original: &str,
    replacement: &str,
    catalog: Option<&str>,
) -> Result<ReplacementCheck> {
    let plan = replacement_plan(state, original, replacement)?;
    let mut next = state.clone();
    apply_replacement(&mut next, original, replacement);
    let issues = next
        .packages
        .iter()
        .filter(|(id, _)| next.is_active(id))
        .flat_map(|(id, p)| {
            deps::status(&next, p)
                .into_iter()
                .filter(|s| !s.satisfied)
                .map(|dependency| ReplacementIssue {
                    consumer: id.clone(),
                    dependency,
                })
        })
        .collect();
    let fixed = deps::fixed_from_state(&next)?;
    // Only the proposed provider's closure is solved. Consumers of ORIGINAL
    // contribute version constraints through plan.incompatible, not unrelated deps.
    let roots = BTreeMap::from([(plan.provider.clone(), fixed[&plan.provider].version.clone())]);
    // The solver pins versions but does not model enabled state. Check installed
    // providers in the same closure explicitly, following replacement bindings.
    let mut pending = vec![plan.provider.clone()];
    let mut visited = BTreeSet::new();
    let mut inactive = Vec::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        for dependency in deps::status(&next, &next.packages[&id]) {
            if let Some(provider) = dependency.provider {
                if !next.packages[&provider].enabled {
                    inactive.push(format!("{id}: {}", dependency.problem.unwrap()));
                }
                pending.push(provider);
            }
        }
    }
    let (missing, resolution_error) = match deps::solve(&fixed, &roots, catalog) {
        Ok(missing) => (
            missing
                .into_iter()
                .map(|(id, v)| (id, v.to_string()))
                .collect(),
            None,
        ),
        Err(error) => (BTreeMap::new(), Some(error.to_string())),
    };
    let (missing, resolution_error) = if inactive.is_empty() {
        (missing, resolution_error)
    } else {
        (BTreeMap::new(), Some(inactive.join("; ")))
    };
    Ok(ReplacementCheck {
        plan,
        issues,
        missing,
        resolution_error,
    })
}

fn replacement_plan(state: &State, original: &str, replacement: &str) -> Result<ReplacementPlan> {
    valid_id(original)?;
    let current = state.replacements.get(original).cloned();
    let (provider, version) = if replacement != original {
        let fork = replacement;
        valid_id(fork)?;
        let installed = state
            .packages
            .get(fork)
            .ok_or_else(|| Error::NotInstalled { id: fork.into() })?;
        // A disabled replacement has no dist/: nothing would be loaded for either.
        ensure!(
            installed.enabled,
            InvalidArgument,
            "{fork} is disabled; enable it first"
        );
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
        // A replacement takes over an installed original.
        if !state.packages.contains_key(original) {
            return Err(Error::NotInstalled {
                id: original.into(),
            });
        }
        (fork.to_owned(), declared.version.clone())
    } else {
        let installed = state
            .packages
            .get(original)
            .ok_or_else(|| Error::NotInstalled {
                id: original.into(),
            })?;
        ensure!(
            installed.enabled,
            InvalidArgument,
            "{original} is disabled; enable it first"
        );
        (original.to_owned(), installed.current.version.clone())
    };
    let consumers: Vec<String> = state
        .packages
        .iter()
        .filter(|(id, p)| {
            **id != provider && state.is_active(id) && p.dependencies.contains_key(original)
        })
        .map(|(id, _)| id.clone())
        .collect();
    let incompatible = consumers
        .iter()
        .filter(|_| state.packages.contains_key(&provider))
        .filter_map(|id| {
            let requirement = &state.packages[id].dependencies[original];
            (!deps::matches(requirement, &version)).then(|| IncompatibleConsumer {
                id: id.clone(),
                requirement: requirement.clone(),
            })
        })
        .collect();
    let installed_version = state
        .packages
        .get(&provider)
        .map(|p| p.current.version.clone());
    Ok(ReplacementPlan {
        original: original.into(),
        provider,
        version,
        current,
        consumers,
        incompatible,
        installed_version,
        dist: DistSync::default(),
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
            self.root.join("worktrees").join(&self.author.id)
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
        /// Run a succeeding operation and prove it left the workspace byte-identical.
        fn refuses_nothing<T: Debug>(&self, op: impl FnOnce() -> Result<T>) -> T {
            let before = tree_hash(&self.root);
            let value = op().expect("operation must succeed");
            assert_eq!(
                tree_hash(&self.root),
                before,
                "workspace changed: {value:?}"
            );
            value
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
        fs::create_dir_all(dir.join("dist/vendor/module/.git")).unwrap();
        let plugins = dir.join("dist");
        assert!(exact_path(&plugins, "vendor/module").unwrap());
        assert!(exact_path(&plugins, "vendor").unwrap());
        for other in ["Vendor/module", "vendor/Module", "vendor/mod", "x"] {
            assert!(!exact_path(&plugins, other).unwrap(), "{other}");
        }
        assert!(!exact_path(&dir.join("missing"), "vendor").unwrap());
    }

    // ---- open -----------------------------------------------------------------------

    #[test]
    fn open_with_validates_options_before_creating_anything() {
        let dir = TempDir::new("workspace");
        let before = tree_hash(dir.path());
        let root = dir.join("nested/new-ws");
        for (location, kind) in [
            ("ftp://example.com/catalog", "invalid_argument"),
            ("http://example.com/catalog", "invalid_argument"),
        ] {
            let options = Options {
                catalog: Some(location.into()),
                custom_ssh: false,
            };
            let error = Manager::open_with(&root, options).err().unwrap();
            assert_eq!(error.kind(), kind, "{location}: {error}");
        }
        assert!(!dir.join("nested").exists());
        assert_eq!(tree_hash(dir.path()), before);
        // A valid remote location is not fetched when opening.
        let options = Options {
            catalog: Some("https://example.invalid/catalog".into()),
            custom_ssh: false,
        };
        drop(Manager::open_with(&root, options).unwrap());
        assert!(root.join("state.json").exists());
    }

    #[test]
    fn a_missing_catalog_location_is_refused_before_creating_anything() {
        let dir = TempDir::new("workspace");
        let before = tree_hash(dir.path());
        let root = dir.join("new-ws");
        for missing in [dir.join("catlog"), dir.join("catalog/catalog.json")] {
            let location = missing.to_string_lossy().into_owned();
            let options = Options {
                catalog: Some(location.clone()),
                custom_ssh: false,
            };
            let error = Manager::open_with(&root, options).err().unwrap();
            assert_eq!(error.kind(), "invalid_argument");
            assert_eq!(
                error.to_string(),
                format!("catalog location {location} does not exist")
            );
        }
        assert!(!root.exists() && !dir.join(".new-ws.lock").exists());
        assert_eq!(tree_hash(dir.path()), before);
    }

    #[test]
    fn open_creates_the_workspace_layout_and_lock() {
        let dir = TempDir::new("workspace");
        let root = dir.join("nested/workspace");
        let catalog = dir.join("catalog-local");
        fs::create_dir(&catalog).unwrap();
        let catalog = catalog.to_string_lossy().into_owned();
        let options = Options {
            catalog: Some(catalog.clone()),
            custom_ssh: false,
        };
        let m = Manager::open_with(&root, options).unwrap();
        assert_eq!(m.root(), root);
        assert_eq!(m.options().catalog.as_deref(), Some(catalog.as_str()));
        assert_eq!(
            read(&root.join("state.json")),
            "{\n  \"format\": 1,\n  \"packages\": {}\n}"
        );
        assert!(!root.join("packages").exists() && root.join("dist").is_dir());
        assert!(!root.join("plugins").exists());
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
    fn open_ds_store_only_root() {
        let dir = TempDir::new("workspace");
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".DS_Store"), b"Finder metadata").unwrap();

        let manager = Manager::open(&root).unwrap();
        assert!(manager.list().unwrap().is_empty());
        assert!(root.join("state.json").is_file());
        assert!(!root.join("packages").exists());
        assert!(root.join("dist").is_dir());
        assert_eq!(
            fs::read(root.join(".DS_Store")).unwrap(),
            b"Finder metadata"
        );
    }

    #[test]
    fn open_ds_store_with_other_content_is_refused() {
        let dir = TempDir::new("workspace");
        for name in ["notes.txt", ".hidden"] {
            let root = dir.join(name);
            fs::create_dir(&root).unwrap();
            fs::write(root.join(".DS_Store"), b"Finder metadata").unwrap();
            fs::write(root.join(name), b"user data").unwrap();
            let before = tree_hash(&root);
            let error = Manager::open(&root).err().unwrap();
            assert_eq!(error.kind(), "workspace");
            assert_eq!(tree_hash(&root), before);
        }
    }

    #[test]
    fn open_retries_after_interrupted_initial_state_write() {
        for existing_root in [false, true] {
            let dir = TempDir::new("workspace-initial-state-crash");
            let root = dir.join("workspace");
            if existing_root {
                fs::create_dir(&root).unwrap();
            }
            crate::atomic_write::CRASH_WRITE.set(true);
            let crash = std::panic::catch_unwind(|| Manager::open(&root));
            assert!(crash.is_err(), "initial state write was not interrupted");
            assert!(
                !root.join("state.json").exists(),
                "partial state was published"
            );
            let staged: Vec<_> = fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert!(
                staged.len() == 1 && is_initial_state_temp(&staged[0]),
                "the root holds only the staged state: {staged:?}"
            );
            // Any other entry makes it a foreign directory: refused, the staged file kept.
            for name in ["notes.txt", ".state.json-1-x.tmp"] {
                fs::write(root.join(name), "user data").unwrap();
                let before = tree_hash(&root);
                assert_eq!(reopen(&root).err().unwrap().kind(), "workspace");
                assert_eq!(tree_hash(&root), before);
                fs::remove_file(root.join(name)).unwrap();
            }
            let manager = reopen(&root).unwrap();
            assert!(manager.list().unwrap().is_empty());
            assert_eq!(
                read(&root.join("state.json")),
                "{\n  \"format\": 1,\n  \"packages\": {}\n}"
            );
            assert!(!root.join("packages").exists() && root.join("dist").is_dir());
            assert!(
                fs::read_dir(&root)
                    .unwrap()
                    .all(|entry| !is_initial_state_temp(&entry.unwrap().file_name())),
                "stale initial state temporary remains after reopen"
            );
        }
    }

    #[test]
    fn open_creates_a_workspace_on_a_mount_point_root() {
        let dir = TempDir::new("workspace-mount-point");
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        crate::atomic_write::MOUNT_POINT.set(Some(root.clone()));
        let opened = Manager::open(&root);
        crate::atomic_write::MOUNT_POINT.set(None);
        assert!(opened.unwrap().list().unwrap().is_empty());
        assert!(root.join("state.json").is_file());
    }

    #[test]
    fn open_ds_store_root_with_abandoned_initial_state() {
        let dir = TempDir::new("workspace-ds-store-crash");
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".DS_Store"), b"Finder metadata").unwrap();
        crate::atomic_write::CRASH_WRITE.set(true);
        let crash = std::panic::catch_unwind(|| Manager::open(&root));
        assert!(crash.is_err(), "initial state write was not interrupted");
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .any(|entry| is_initial_state_temp(&entry.unwrap().file_name()))
        );

        let manager = reopen(&root).unwrap();
        assert!(manager.list().unwrap().is_empty());
        assert!(root.join("state.json").is_file());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !is_initial_state_temp(&entry.unwrap().file_name())),
            "stale initial state temporary remains after reopen"
        );
        assert_eq!(
            fs::read(root.join(".DS_Store")).unwrap(),
            b"Finder metadata"
        );
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
                r#"{"format":1,"packages":{"../evil":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true,"metadata":{}}}}"#,
                "invalid_id",
            ),
            (
                r#"{"format":1,"packages":{"a/b/c":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true,"metadata":{}}}}"#,
                "invalid_id",
            ),
            (
                r#"{"format":1,"packages":{"demo1":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true,"metadata":{}}}}"#,
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
        // Two bare stores, two worktrees, two payloads and state: thirteen renames.
        for renames in 0..13 {
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
                let installed = &state.packages[&author.id];
                assert_eq!(installed.current.version, "0.2.0", "crash after {renames}");
                assert_eq!(installed.previous.as_ref().unwrap().version, "0.1.0");
                assert_eq!(installed.current.commit, author.head());
                let checkout = ws.root.join("worktrees").join(&author.id);
                assert_eq!(head_of(&checkout), installed.current.commit);
                assert!(fixture_git(&checkout, &["status", "--porcelain"]).is_empty());
                assert_eq!(
                    tree_hash(&ws.root.join("dist").join(&author.id)),
                    tree_hash(&author.path.join("dist"))
                );
            }
            ws.no_leftovers();
        }
    }

    #[test]
    fn disable_and_enable_roll_dist_forward_after_an_interrupted_commit() {
        let id = "dan/demo1";
        // disable: dist out, state.json; enable: dist in, state.json.
        for renames in 0..2 {
            let (ws, m) = Ws::installed(id);
            crash_after(renames, || m.disable(id));
            drop(m);
            let m = ws.open();
            assert!(
                !m.state().unwrap().packages[id].enabled,
                "crash after {renames}"
            );
            assert!(!ws.dist().exists(), "crash after {renames}");
            assert!(
                !ws.root.join("dist/dan").exists(),
                "empty namespace removed"
            );
            ws.no_leftovers();
            crash_after(renames, || m.enable(id));
            drop(m);
            let m = ws.open();
            assert!(
                m.state().unwrap().packages[id].enabled,
                "crash after {renames}"
            );
            assert_eq!(
                tree_hash(&ws.dist()),
                tree_hash(&ws.author.path.join("dist"))
            );
            ws.no_leftovers();
            assert_eq!(m.sync_dist_plan().unwrap(), DistSync::default());
        }
    }

    #[test]
    fn a_reconcile_that_deploys_and_removes_rolls_forward_after_an_interrupted_commit() {
        // dist/x/a in (1), dist/x/b out (1), state.json (1).
        for renames in 0..3 {
            let (ws, m) = Ws::installed("x/a");
            let other = ws.other_author("other", "x/b");
            m.install(other.source()).unwrap();
            m.disable("x/b").unwrap();
            fs::remove_dir_all(ws.root.join("dist/x/a")).unwrap();
            copy_tree(&other.path.join("dist"), &ws.root.join("dist/x/b"), false).unwrap();
            let plan = m.sync_dist_plan().unwrap();
            assert_eq!(
                (plan.deployed, plan.removed),
                (vec!["x/a".into()], vec!["x/b".into()])
            );
            crash_after(renames, || m.sync_dist());
            drop(m);
            let m = ws.open();
            assert_eq!(
                tree_hash(&ws.root.join("dist/x/a")),
                tree_hash(&ws.author.path.join("dist")),
                "crash after {renames}"
            );
            assert!(!ws.root.join("dist/x/b").exists(), "crash after {renames}");
            ws.no_leftovers();
            assert_eq!(m.sync_dist_plan().unwrap(), DistSync::default());
        }
    }

    #[test]
    fn a_revision_without_dist_is_never_deployed_empty() {
        let (ws, m) = Ws::installed("dan/demo1");
        fixture_git(&ws.author.path, &["rm", "-q", "-r", "dist"]);
        fixture_git(&ws.author.path, &["commit", "-q", "-m", "no dist"]);
        let commit = ws.author.head();
        let target = ws.dir.join("target");
        let error = export_dist(&ws.author.path, &commit, None, &target, false).unwrap_err();
        assert_eq!(error.kind(), "invalid_manifest", "{error}");
        assert!(error.to_string().contains("has no dist/ folder"), "{error}");
        assert!(!target.exists());
        drop(m);
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
            assert_eq!(
                p.current.as_ref().unwrap().version,
                "0.2.0",
                "crash after {renames}"
            );
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
        fs::create_dir_all(
            ws.root
                .join(txn::DIR)
                .join("new/repos/half-0123456789abcdef"),
        )
        .unwrap();
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
            ws.refuses("invalid_id", || m.update_to(id, Target::Dev));
            ws.refuses("invalid_id", || m.rollback(id));
            ws.refuses("invalid_id", || m.enable(id));
            ws.refuses("invalid_id", || m.disable(id));
            ws.refuses("invalid_id", || m.forget(id));
            ws.refuses("invalid_id", || m.restore(id));
            ws.refuses("invalid_id", || m.uninstall_plan(id));
            ws.refuses("invalid_id", || m.uninstall(id, &[], false));
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
            ws.refuses("not_installed", || {
                m.update_to(id, Target::Ref("HEAD".into()))
            }),
            ws.refuses("not_installed", || m.update_to(id, Target::Dev)),
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
            assert_eq!(error.to_string(), format!("package {id} is not installed"));
        }
        for error in [
            ws.refuses("repository_not_found", || m.uninstall_plan(id)),
            ws.refuses("repository_not_found", || m.uninstall(id, &[], false)),
            ws.refuses("repository_not_found", || {
                m.uninstall(id, &[LostWork::Untracked { path: "x".into() }], true)
            }),
        ] {
            assert!(matches!(error, Error::RepositoryNotFound { .. }));
        }
        // A namespace is not a package ID.
        ws.refuses("invalid_id", || m.uninstall("dan", &[], false));
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
        fs::write(ws.root.join("state.json"), "{\"packages\": {").unwrap();
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
        ws.refuses("invalid_data", || m.uninstall(id, &[], false));
        ws.refuses("invalid_data", || {
            m.fork_in_namespace(id, &ws.dir.join("f"), "me")
        });
        assert_eq!(read(&ws.root.join("state.json")), "{\"packages\": {");
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
                commit: c110.clone(),
                ..Default::default()
            }
        );
        assert!(result.fork_of.is_none());

        let state = m.state().unwrap();
        let p = &state.packages["dan/demo1"];
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
            (
                list.len(),
                list[0].current.as_ref().unwrap().version.as_str()
            ),
            (1, "1.10.0")
        );
        ws.no_leftovers();
    }

    #[test]
    fn install_by_ref_and_default_branch() {
        let ws = Ws::new("dan/demo1");
        let first = ws.author.head();
        ws.author.release("0.2.0");
        let head = ws.author.commit("0.3.0");
        let m = ws.open();
        let r = m
            .install_package(
                ws.author.source(),
                None,
                InstallTarget::Ref("v0.1.0".into()),
            )
            .unwrap();
        assert_eq!(
            (r.id.as_str(), r.current.commit.as_str()),
            ("dan/demo1", first.as_str())
        );
        assert!(ws.repo().join(".git").is_file());
        assert!(ws.root.join("dist/dan/demo1/index.js").is_file());
        m.uninstall("dan/demo1", &[], false).unwrap();
        let r = m
            .install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap();
        assert_eq!(
            (r.current.version.as_str(), r.current.commit),
            ("0.3.0", head)
        );
    }

    #[test]
    fn install_refusals_leave_the_workspace_unchanged() {
        let (ws, m) = Ws::installed("dan/demo1");
        // The same source again: reported, nothing changed.
        let before = tree_hash(&ws.root);
        let again = m.install(ws.author.source()).unwrap();
        assert!(
            again.already_installed && again.available.is_none(),
            "{again:?}"
        );
        assert_eq!(
            again.current,
            m.state().unwrap().packages["dan/demo1"].current
        );
        assert_eq!(tree_hash(&ws.root), before);
        ws.no_leftovers();
        ws.refuses("invalid_argument", || m.install("-uevil"));
        ws.refuses("invalid_argument", || {
            m.install_package(ws.author.source(), None, InstallTarget::Ref("-x".into()))
        });
        ws.refuses("git", || {
            m.install(ws.dir.join("missing").to_str().unwrap())
        });
        ws.refuses("invalid_argument", || {
            m.install_package(
                ws.author.source(),
                None,
                InstallTarget::Ref("no-such-ref".into()),
            )
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
            m.install_package(deps.source(), None, InstallTarget::Ref("HEAD".into()))
        });
        assert!(error.to_string().contains("dan/missing"), "{error}");

        let no_dist = Author::new(ws.dir.join("nodist"), "dan/nodist");
        fixture_git(&no_dist.path, &["rm", "-q", "-r", "dist"]);
        write_manifest(&no_dist.path, &make_manifest("dan/nodist", "0.2.0"));
        fixture_git(&no_dist.path, &["commit", "-q", "-am", "no dist"]);
        ws.refuses("invalid_manifest", || {
            m.install_package(no_dist.source(), None, InstallTarget::Ref("HEAD".into()))
        });

        // An unscoped ID is refused, also when it equals an installed namespace.
        let unscoped = ws.other_author("unscoped", "dan");
        ws.refuses("invalid_id", || m.install(unscoped.source()));
        let solo = ws.other_author("solo", "solo");
        ws.refuses("invalid_id", || m.install(solo.source()));

        // The expected manifest must match the Git revision exactly.
        let other = ws.other_author("other", "dan/other");
        let wrong = catalog::Release {
            manifest: make_manifest("dan/other", "9.9.9"),
            source: other.source().into(),
            commit: other.head(),
            reference: "v9.9.9".into(),
            files: catalog::hash_files(&other.path.join("dist")).unwrap(),
            build: None,
            path: None,
        };
        let error = ws.refuses("catalog", || m.install_release(&wrong));
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
            reference: "v0.1.0".into(),
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
        assert!(ws.repo().join(".git").is_file() && !ws.dist().exists());
        let error = ws.refuses("already_installed", || m.install(ws.author.source()));
        assert!(
            error
                .to_string()
                .contains("was forgotten and its clone is kept"),
            "{error}"
        );
        ws.refuses("not_installed", || m.forget("dan/demo1"));
        let restored = m.restore("dan/demo1").unwrap();
        assert_eq!(restored.source, ws.author.source());
        assert_eq!(restored.current.version, "0.1.0");
        let p = &m.state().unwrap().packages["dan/demo1"];
        assert!(p.enabled && p.previous.is_none());
        assert!(ws.dist().join("index.js").is_file());
        let error = ws.refuses("already_installed", || m.restore("dan/demo1"));
        assert_eq!(error.to_string(), "package dan/demo1 is already registered");
    }

    #[test]
    fn restore_of_a_recorded_package_is_already_installed() {
        let (ws, m) = Ws::installed("dan/demo1");
        // Restore works on forgotten records only, even when the clone is gone.
        fs::remove_dir_all(ws.repo()).unwrap();
        let error = ws.refuses("already_installed", || m.restore("dan/demo1"));
        assert_eq!(error.to_string(), "package dan/demo1 is already registered");
    }

    #[test]
    fn restore_refuses_dirty_clones_and_needs_a_forgotten_record() {
        let (ws, m) = Ws::installed("dan/demo1");
        m.forget("dan/demo1").unwrap();
        fs::write(ws.repo().join("dist/index.js"), "local edit").unwrap();
        let error = ws.refuses("local_changes", || m.restore("dan/demo1"));
        assert!(matches!(error, Error::LocalChanges { items, .. } if items.len() == 1));
        fixture_git(&ws.repo(), &["checkout", "-q", "--", "dist/index.js"]);
        // A clone without a forgotten record is not restorable.
        copy_tree(
            &ws.repo(),
            &ws.root.join("repos/unrecorded-0123456789abcdef"),
            true,
        )
        .unwrap();
        ws.refuses("repository_not_found", || m.restore("dan/alias2"));
        // A forgotten clone that is gone: the record is all that is left.
        fs::remove_dir_all(ws.repo()).unwrap();
        ws.refuses("repository_not_found", || m.restore("dan/demo1"));
        m.uninstall("dan/demo1", &[], false).unwrap();
        assert!(m.state().unwrap().forgotten.is_empty());
    }

    #[test]
    fn restore_keeps_the_forgotten_record() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        ws.author.release("0.2.0");
        catalog::register(&location, ws.author.source(), "v0.2.0").unwrap();
        // A verified catalog records `catalog`, which restore keeps.
        crate::test_support::verify_catalog(&catalog_dir);
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location),
                custom_ssh: false,
            },
        )
        .unwrap();
        // Source-keyed clone from a catalog, with a previous revision, disabled, then forgotten.
        m.install_from_catalog(id, Some("0.1.0")).unwrap();
        m.update(id).unwrap();
        m.disable(id).unwrap();
        let before = serde_json::to_value(&m.state().unwrap().packages[id]).unwrap();
        assert!(before["catalog"].is_string() && before["previous"].is_object());
        m.forget(id).unwrap();
        let state = m.state().unwrap();
        assert!(state.packages.is_empty());
        assert_eq!(serde_json::to_value(&state.forgotten[id]).unwrap(), before);
        let restored = m.restore(id).unwrap();
        assert_eq!(restored.current.version, "0.2.0");
        let state = m.state().unwrap();
        assert!(state.forgotten.is_empty());
        assert_eq!(serde_json::to_value(&state.packages[id]).unwrap(), before);
        // Disabled: nothing is deployed.
        assert!(restored.dist.deployed.is_empty() && !ws.dist().exists());

        // A HEAD moved to another published commit is local work for a sole user.
        m.enable(id).unwrap();
        // A protected package follows a ref only once detached.
        m.update_with(
            id,
            Target::Ref("v0.1.0".into()),
            UpdateOptions { detach: true },
        )
        .unwrap();
        m.forget(id).unwrap();
        fixture_git(&ws.repo(), &["checkout", "-q", "--detach", "v0.2.0"]);
        let error = ws.refuses("local_changes", || m.restore(id));
        assert!(
            matches!(&error, Error::LocalChanges { items, .. }
                if matches!(items.as_slice(), [LostWork::HeadMoved { .. }])),
            "{error:?}"
        );
        fixture_git(&ws.repo(), &["checkout", "-q", "--detach", "v0.1.0"]);
        let restored = m.restore(id).unwrap();
        assert_eq!(restored.dist.deployed, [id]);
        let p = &m.state().unwrap().packages[id];
        assert_eq!(p.mode, Mode::Pinned("v0.1.0".into()));
        assert_eq!(p.current.version, "0.1.0");
        assert!(p.enabled && p.catalog.is_none());
        assert!(ws.dist().join("index.js").is_file());
    }

    #[test]
    fn restore_refuses_an_unpublished_commit_in_every_mode() {
        let id = "dan/demo1";
        for target in [InstallTarget::Release, InstallTarget::Dev] {
            let ws = Ws::new(id);
            let m = ws.open();
            m.install_package(ws.author.source(), None, target.clone())
                .unwrap();
            m.forget(id).unwrap();
            fs::write(ws.repo().join("dist/index.js"), "local").unwrap();
            fixture_git(&ws.repo(), &["commit", "-q", "-am", "local"]);
            let error = ws.refuses("local_changes", || m.restore(id));
            assert!(
                matches!(&error, Error::LocalChanges { items, .. }
                    if items.iter().any(|i| matches!(i, LostWork::HeadMoved { .. }))),
                "{target:?}: {error:?}"
            );
        }
    }

    #[test]
    fn branch_mode_restore_refuses_a_published_moved_head() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        let recorded = m
            .install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap()
            .current;
        m.forget(id).unwrap();
        ws.author.commit("0.2.0");
        fixture_git(&ws.repo(), &["fetch", "-q", "origin"]);
        fixture_git(&ws.repo(), &["checkout", "-q", "--detach", "origin/main"]);
        let error = ws.refuses("local_changes", || m.restore(id));
        assert!(
            matches!(error, Error::LocalChanges { items, .. } if items.iter().any(|item| matches!(item, LostWork::HeadMoved { .. })))
        );
        assert_eq!(m.state().unwrap().forgotten[id].current, recorded);
    }

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
        let p = m.state().unwrap().packages[id].clone();
        assert_eq!(p.current.version, "0.2.0");
        assert_eq!(p.previous.as_ref().unwrap().commit, v1);
        assert!(!p.enabled, "update keeps the enabled flag");
        assert!(!ws.dist().exists(), "a disabled package has no dist");
        m.enable(id).unwrap();
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
        let p = m.state().unwrap().packages[id].clone();
        assert_eq!(p.previous.unwrap().version, "0.2.0");
        let forward = m.rollback(id).unwrap();
        assert_eq!(forward.to.commit, v2);
        // Fetching from the missing origin fails without changes.
        ws.refuses("git", || m.update(id));
    }

    #[test]
    fn major_updates_are_marked() {
        let (ws, m) = Ws::installed("dan/demo1");
        let id = "dan/demo1";
        // 0.x: a new PATCH is compatible, a new MINOR is major.
        ws.author.release("0.1.1");
        let change = m.update(id).unwrap();
        assert!(change.changed && !change.major, "{change:?}");
        ws.author.release("0.2.0");
        assert!(m.update(id).unwrap().major);
        // Across a major version, not held back by itself.
        ws.author.release("1.0.0");
        ws.author.release("1.1.0");
        ws.author.release("2.0.0");
        let change = m.update(id).unwrap();
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("0.2.0", "2.0.0")
        );
        assert!(change.major);
        assert_eq!(
            serde_json::to_value(&change).unwrap()["major"],
            serde_json::json!(true)
        );
        // A rollback is never major.
        assert!(!m.rollback(id).unwrap().major);
        m.rollback(id).unwrap();
        ws.author.release("2.1.0");
        let plan = m.update_all().unwrap();
        assert_eq!(
            plan.updates
                .iter()
                .map(|c| (c.to.version.as_str(), c.major))
                .collect::<Vec<_>>(),
            [("2.1.0", false)]
        );
        ws.author.release("3.0.0");
        let plan = m.update_all_plan().unwrap();
        assert!(plan.updates[0].major, "{plan:?}");
        assert!(!m.update(id).unwrap().to.version.is_empty());
        assert!(
            !m.update(id).unwrap().major,
            "an unchanged update is not major"
        );
    }

    #[test]
    fn pre_releases_are_opt_in() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        // A newer pre-release is never picked implicitly, nor reported as broken.
        ws.author.release("1.3.0-beta.1");
        let noop = m.update(id).unwrap();
        assert!(!noop.changed, "{noop:?}");
        assert_eq!(noop.to.version, "0.1.0");
        let plan = m.update_all_plan().unwrap();
        assert!(
            plan.updates.is_empty() && plan.kept_back.is_empty(),
            "{plan:?}"
        );
        let rows = m.check().unwrap();
        assert_eq!(
            (
                rows[0].status,
                rows[0].latest.as_deref(),
                rows[0].prerelease.as_deref()
            ),
            (
                check::CheckStatus::UpToDate,
                Some("0.1.0"),
                Some("1.3.0-beta.1")
            )
        );
        // A fresh install takes the newest release too.
        let other = Manager::open(ws.dir.join("other")).unwrap();
        assert_eq!(
            other.install(ws.author.source()).unwrap().current.version,
            "0.1.0"
        );
        // Named explicitly, it is installed (a tag ref pins).
        let tag = identity::release_tag("1.3.0-beta.1");
        let change = m.update_to(id, Target::Ref(tag.clone())).unwrap();
        assert_eq!(change.to.version, "1.3.0-beta.1");
        assert!(
            change.major,
            "0.1.0 → 1.3.0-beta.1 crosses a breaking boundary"
        );
        // Back in release mode it follows its own track: newer pre-releases of 1.3.0
        // and the release, never another version's pre-release.
        ws.author.release("1.3.0-beta.2");
        ws.author.release("1.4.0-beta.1");
        let change = m.update_to(id, Target::Release).unwrap();
        assert_eq!(change.to.version, "1.3.0-beta.2");
        assert_eq!(change.mode, Mode::Release);
        assert!(!m.update(id).unwrap().changed);
        assert!(m.update_all_plan().unwrap().updates.is_empty());
        ws.author.release("1.3.0");
        let change = m.update(id).unwrap();
        assert_eq!((change.to.version.as_str(), change.major), ("1.3.0", false));
        let noop = m.update(id).unwrap();
        assert!(!noop.changed, "{noop:?}");
        // The track ends at the newest release, which may be past its own version.
        let (ws, m) = Ws::installed(id);
        ws.author.release("1.3.0-beta.1");
        m.update_to(id, Target::Ref(identity::release_tag("1.3.0-beta.1")))
            .unwrap();
        ws.author.release("1.3.0-beta.2");
        m.update_to(id, Target::Release).unwrap();
        ws.author.release("1.4.0");
        let change = m.update(id).unwrap();
        assert_eq!(
            (
                change.from.version.as_str(),
                change.to.version.as_str(),
                change.major
            ),
            ("1.3.0-beta.2", "1.4.0", false)
        );
        // update all follows the track of an installed pre-release as well.
        let (ws, m) = Ws::installed(id);
        ws.author.release("2.0.0-rc.1");
        m.update_to(id, Target::Ref(identity::release_tag("2.0.0-rc.1")))
            .unwrap();
        m.update_to(id, Target::Release).unwrap();
        ws.author.release("2.0.0-rc.2");
        ws.author.release("2.1.0-rc.1");
        let plan = m.update_all().unwrap();
        assert_eq!(plan.updates[0].to.version, "2.0.0-rc.2", "{plan:?}");
        // A repository with pre-release tags only has nothing to install implicitly.
        let beta = ws.other_author("beta", "dan/beta");
        fixture_git(&beta.path, &["tag", "-d", "v0.1.0"]);
        beta.release("1.0.0-beta.1");
        let error = ws.refuses("no_releases", || m.install(beta.source()));
        assert!(error.to_string().contains("only pre-release"), "{error}");
        let installed = m
            .install_package(
                beta.source(),
                None,
                InstallTarget::Ref(identity::release_tag("1.0.0-beta.1")),
            )
            .unwrap();
        assert_eq!(installed.current.version, "1.0.0-beta.1");
    }

    #[test]
    fn rollback_manifest_mismatch_names_the_package() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        ws.author.release("0.2.0");
        m.update(id).unwrap();
        // Corrupt the recorded previous version, while its commit still exists.
        let path = ws.root.join("state.json");
        let mut state: serde_json::Value = serde_json::from_str(&read(&path)).unwrap();
        state["packages"][id]["previous"]["version"] = "0.0.9".into();
        fs::write(&path, state.to_string()).unwrap();
        let error = ws.refuses("invalid_manifest", || m.rollback(id));
        assert_eq!(error.to_string(), "dan/demo1: rollback manifest mismatch");
        assert!(matches!(error, Error::InvalidManifest { id: Some(package), .. } if package == id));
        // Point the previous revision at a valid commit declaring another package.
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.3.0"));
        fixture_git(&ws.author.path, &["commit", "-qam", "rename"]);
        let renamed = ws.author.head();
        fixture_git(&ws.repo(), &["fetch", ws.author.source(), &renamed]);
        state["packages"][id]["previous"]["commit"] = renamed.into();
        state["packages"][id]["previous"]["version"] = "0.3.0".into();
        fs::write(&path, state.to_string()).unwrap();
        let error = ws.refuses("invalid_manifest", || m.rollback(id));
        assert_eq!(error.to_string(), "dan/demo1: rollback manifest mismatch");
        assert!(matches!(error, Error::InvalidManifest { id: Some(package), .. } if package == id));
    }

    #[test]
    fn update_all_upstream_identity_refusal_names_the_package() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap();
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.2.0"));
        fixture_git(&ws.author.path, &["commit", "-qam", "rename"]);
        for error in [
            ws.refuses("invalid_manifest", || m.update_all()),
            ws.refuses("invalid_manifest", || m.update_all_plan()),
        ] {
            assert_eq!(
                error.to_string(),
                "dan/demo1: package id changed upstream to dan/renamed"
            );
            assert!(
                matches!(error, Error::InvalidManifest { id: Some(package), .. } if package == id)
            );
        }
    }

    #[test]
    fn update_all_local_work_names_the_package_and_items() {
        let (ws, m) = Ws::installed("dan/demo1");
        let repo = ws.repo();
        fs::write(repo.join("local.txt"), "local work").unwrap();
        fixture_git(&repo, &["add", "local.txt"]);
        fixture_git(&repo, &["commit", "-qm", "local"]);
        fixture_git(&repo, &["tag", "local-only"]);
        fixture_git(&repo, &["reset", "--hard", "HEAD~1"]);
        let error = ws.refuses("local_changes", || m.update_all());
        assert_eq!(
            error.to_string(),
            "dan/demo1: unpublished local work blocks update all; unpublished tag: local-only"
        );
        assert!(matches!(error, Error::LocalChanges { id, items, .. }
            if id == "dan/demo1" && items == [LostWork::UnpublishedTag { name: "local-only".into() }]));
    }

    #[test]
    fn update_refusals_leave_the_workspace_unchanged() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        ws.refuses("no_previous_version", || m.rollback(id));
        ws.refuses("invalid_argument", || {
            m.update_to(id, Target::Ref("-x".into()))
        });

        // Upstream: same version on a new commit, then a commit with another ID.
        // Tags are not policed: an explicit commit may keep the installed version.
        let same = same_version_commit(&ws.author);
        let change = m.update_to(id, Target::Ref(String::from(&same))).unwrap();
        assert_eq!(change.to.commit, same);
        assert_eq!(change.mode, Mode::Pinned(same.clone()));
        assert!(change.warnings.is_empty(), "{:?}", change.warnings);
        m.rollback(id).unwrap();
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.2.0"));
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "rename"]);
        let renamed = ws.author.head();
        let error = ws.refuses("invalid_manifest", || {
            m.update_to(id, Target::Ref(String::from(&renamed)))
        });
        assert_eq!(
            error.to_string(),
            "dan/demo1: package id changed upstream to dan/renamed"
        );
        assert!(matches!(error, Error::InvalidManifest { id: Some(package), .. } if package == id));
    }

    /// New upstream commit that keeps the manifest version.
    fn same_version_commit(author: &Author) -> String {
        fs::write(author.path.join("dist/extra.js"), "extra").unwrap();
        fixture_git(&author.path, &["add", "--all"]);
        fixture_git(&author.path, &["commit", "-q", "-m", "same version"]);
        author.head()
    }

    #[test]
    fn return_to_releases_at_the_installed_version_keeps_the_commit() {
        let ws = Ws::new("dan/demo1");
        let pinned = same_version_commit(&ws.author);
        let m = ws.open();
        // Pinned to 0.1.0 at HEAD, but the 0.1.0 release tag is another commit.
        m.install_package(
            ws.author.source(),
            None,
            InstallTarget::Ref(ws.author.head()),
        )
        .unwrap();
        // Versions only: the installed revision stays, only the mode changes.
        let change = m.update_to("dan/demo1", Target::Release).unwrap();
        assert!(change.changed);
        assert_eq!(change.mode, Mode::Release);
        assert_eq!(change.to.commit, pinned);
        assert_eq!(change.warnings.len(), 1, "{:?}", change.warnings);
        assert!(
            change.warnings[0].starts_with("release tag v0.1.0 now points to "),
            "{:?}",
            change.warnings
        );
        assert!(change.warnings[0].ends_with("; the installed commit is kept"));
    }

    #[test]
    fn explicit_ref_to_the_moved_installed_tag_is_accepted() {
        let (ws, m) = Ws::installed("dan/demo1");
        let moved = same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "v0.1.0"]);
        let change = m
            .update_to("dan/demo1", Target::Ref("v0.1.0".into()))
            .unwrap();
        assert!(change.changed);
        assert_eq!(change.to.commit, moved);
        assert_eq!(change.mode, Mode::Pinned("v0.1.0".into()));
        assert_eq!(
            change.warnings,
            [format!("release tag v0.1.0 now points to {}", &moved[..12])]
        );
    }

    #[test]
    fn update_takes_a_moved_release_tag_at_its_new_commit() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.author.release("0.1.1");
        let original = ws.author.release("0.1.2");
        m.update_to("dan/demo1", Target::Ref("v0.1.1".into()))
            .unwrap();
        assert_eq!(resolve_ref(&ws.repo(), "v0.1.2", false).unwrap(), original);
        let moved = same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "v0.1.2"]);
        // The explicit ref pinned it: back to releases, or to the moved tag, both take
        // the tag where it is now.
        for target in [Target::Release, Target::Ref("v0.1.2".into())] {
            let change = m.update_to("dan/demo1", target.clone()).unwrap();
            assert_eq!(change.to.commit, moved);
            assert_eq!(resolve_ref(&ws.repo(), "v0.1.2", false).unwrap(), moved);
            m.rollback("dan/demo1").unwrap();
        }
    }

    #[test]
    fn stable_update_ignores_repository_tag_with_another_manifest_id() {
        let (ws, m) = Ws::installed("dan/demo1");
        ws.author.commit("0.1.1");
        write_manifest(&ws.author.path, &make_manifest("dan/renamed", "0.1.1"));
        fixture_git(&ws.author.path, &["add", "."]);
        fixture_git(&ws.author.path, &["commit", "-qm", "rename"]);
        ws.author.tag("0.1.1");
        let change = m.update("dan/demo1").unwrap();
        assert!(!change.changed);
        assert_eq!(change.to.version, "0.1.0");
        assert!(change.skipped.is_empty());
        assert!(change.warnings.is_empty());
    }

    #[test]
    fn stable_update_follows_deleted_and_moved_upstream_tags() {
        let (ws, m) = Ws::installed("dan/demo1");
        let installed = ws.author.release("0.2.0");
        m.update("dan/demo1").unwrap();
        let state = read(&ws.root.join("state.json"));
        // Moved release tag: same version, other commit; the clone mirrors it.
        let moved = same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "v0.2.0"]);
        let change = m.update("dan/demo1").unwrap();
        assert!(!change.changed);
        assert_eq!(change.to.commit, installed);
        assert_eq!(
            change.warnings,
            [format!(
                "release tag v0.2.0 now points to {}; the installed commit is kept",
                &moved[..12]
            )]
        );
        assert_eq!(resolve_ref(&ws.repo(), "v0.2.0", false).unwrap(), moved);
        assert_eq!(read(&ws.root.join("state.json")), state);
        // Deleted release tag: the installed release was retracted, the package stays.
        fixture_git(&ws.author.path, &["tag", "-d", "v0.2.0"]);
        let change = m.update("dan/demo1").unwrap();
        assert!(!change.changed);
        assert_eq!(
            change.warnings,
            ["installed release dan/demo1@0.2.0 no longer exists on the origin"]
        );
        assert!(resolve_ref(&ws.repo(), "refs/tags/v0.2.0", false).is_err());
        assert_eq!(read(&ws.root.join("state.json")), state);
    }

    #[test]
    fn a_broken_installed_release_tag_is_named_as_skipped() {
        let (ws, m) = Ws::installed("dan/demo1");
        let installed = ws.author.release("0.2.0");
        m.update("dan/demo1").unwrap();
        // The 0.2.0 tag now points at a commit whose manifest says 0.3.0.
        let broken = ws.author.commit("0.3.0");
        fixture_git(&ws.author.path, &["tag", "-f", "v0.2.0"]);
        let plan = m.update_all().unwrap();
        assert_eq!(
            plan.unchanged[0].warnings,
            [format!(
                "release tag v0.2.0 now points to {} (skipped: tag v0.2.0 disagrees with manifest version 0.3.0); the installed commit is kept",
                &broken[..12]
            )]
        );
        assert_eq!(
            m.state().unwrap().packages["dan/demo1"].current.commit,
            installed
        );
        // The one tag refusal left: a broken newest tag is never silently passed over.
        let error = ws.refuses("version_conflict", || m.update("dan/demo1"));
        assert!(
            error
                .to_string()
                .starts_with("dan/demo1: latest release tag is broken")
        );
    }

    #[test]
    fn moved_and_deleted_tags_are_taken_as_they_are() {
        // Source-keyed clone: update all reports what update ID reports, and never fails.
        let (ws, m) = Ws::installed("dan/demo1");
        let installed = ws.author.release("0.2.0");
        m.update("dan/demo1").unwrap();
        let moved = same_version_commit(&ws.author);
        fixture_git(&ws.author.path, &["tag", "-f", "v0.2.0"]);
        for plan in [m.update_all_plan().unwrap(), m.update_all().unwrap()] {
            assert!(plan.updates.is_empty());
            assert_eq!(
                plan.unchanged,
                [Unchanged {
                    id: "dan/demo1".into(),
                    version: "0.2.0".into(),
                    reason: None,
                    warnings: vec![format!(
                        "release tag v0.2.0 now points to {}; the installed commit is kept",
                        &moved[..12]
                    )],
                }]
            );
        }
        // Every release tag deleted on the origin: still up to date, with the warning.
        fixture_git(&ws.author.path, &["tag", "-d", "v0.2.0"]);
        fixture_git(&ws.author.path, &["tag", "-d", "v0.1.0"]);
        let retracted = ["installed release dan/demo1@0.2.0 no longer exists on the origin"];
        let plan = m.update_all().unwrap();
        assert_eq!(plan.unchanged[0].warnings, retracted);
        let change = m.update("dan/demo1").unwrap();
        assert!(!change.changed);
        assert_eq!(change.to.commit, installed);
        assert_eq!(change.warnings, retracted);
        // A new release after the retraction is an ordinary update.
        ws.author.release("0.3.0");
        let change = m.update("dan/demo1").unwrap();
        assert_eq!(change.to.version, "0.3.0");
        assert!(change.warnings.is_empty());
    }

    #[test]
    fn direct_install_reports_skipped_broken_release_tags() {
        let ws = Ws::new("dan/demo1");
        // An old tag whose manifest disagrees with the tag version.
        fixture_git(&ws.author.path, &["tag", "v0.0.9"]);
        ws.author.release("0.2.0");
        let result = ws.open().install(ws.author.source()).unwrap();
        assert_eq!(result.current.version, "0.2.0");
        let names: Vec<_> = result.skipped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["v0.0.9"]);
    }

    #[test]
    fn stable_update_never_downgrades() {
        let ws = Ws::new("dan/demo1");
        ws.author.release("0.5.0");
        let m = ws.open();
        m.install(ws.author.source()).unwrap();
        // The installed release disappears upstream: following releases never goes down.
        fixture_git(&ws.author.path, &["tag", "-d", "v0.5.0"]);
        let change = m.update("dan/demo1").unwrap();
        assert!(!change.changed);
        assert_eq!(change.to.version, "0.5.0");
        assert_eq!(
            change.warnings,
            ["installed release dan/demo1@0.5.0 no longer exists on the origin"]
        );
        assert!(m.update_all().unwrap().updates.is_empty());
        // An explicit return to releases may go down to the newest one (here from branch
        // mode above every release).
        m.update_to("dan/demo1", Target::Dev).unwrap();
        let change = m.update_to("dan/demo1", Target::Release).unwrap();
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("0.5.0", "0.1.0")
        );
        m.rollback("dan/demo1").unwrap();
        // An explicit ref may downgrade.
        let change = m
            .update_to("dan/demo1", Target::Ref("v0.1.0".into()))
            .unwrap();
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
    fn update_all_invalid_requirement_on_the_branch_names_the_package() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap();
        let mut next = make_manifest(id, "0.2.0");
        next.dependencies
            .insert("dan/lib".into(), "^1 || ^2".into());
        write_manifest(&ws.author.path, &next);
        fixture_git(&ws.author.path, &["commit", "-qam", "bad requirement"]);
        for error in [
            ws.refuses("invalid_manifest", || m.update_all()),
            ws.refuses("invalid_manifest", || m.update_all_plan()),
        ] {
            assert!(
                error
                    .to_string()
                    .starts_with("dan/demo1: invalid version requirement"),
                "{error}"
            );
            assert!(
                matches!(error, Error::InvalidManifest { id: Some(package), .. } if package == id)
            );
        }
    }

    #[test]
    fn branch_mode_follows_origin_head_and_refuses_diverged_history() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap();
        let next = ws.author.commit("0.2.0");
        // The recorded mode is followed: no target needed.
        let change = m.update(id).unwrap();
        assert_eq!(
            (change.to.commit.as_str(), change.to.version.as_str()),
            (next.as_str(), "0.2.0")
        );
        // In branch mode a new commit is an update even without a version bump.
        let same = same_version_commit(&ws.author);
        let change = m.update(id).unwrap();
        assert!(change.changed);
        assert_eq!(
            (change.to.commit.as_str(), change.to.version.as_str()),
            (same.as_str(), "0.2.0")
        );
        m.rollback(id).unwrap();
        m.update(id).unwrap();
        // Rewrite upstream history: the installed commit is no longer an ancestor.
        fixture_git(&ws.author.path, &["reset", "-q", "--hard", "HEAD~1"]);
        ws.author.commit("0.3.0");
        for target in [Target::Recorded, Target::Dev] {
            let error = ws.refuses("version_conflict", || m.update_to(id, target.clone()));
            assert!(
                error
                    .to_string()
                    .starts_with("dan/demo1: history of main diverged; automatic update refused"),
                "{error}"
            );
            assert!(
                matches!(error, Error::VersionConflict { id: Some(package), .. } if package == id)
            );
        }
        // An explicit ref is the user's choice: no ancestry needed.
        let head = ws.author.head();
        let change = m.update_to(id, Target::Ref("main".into())).unwrap();
        assert_eq!(change.to.commit, head);
    }

    #[test]
    fn update_to_a_branch_uses_the_fetched_upstream_branch() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        let next = ws.author.commit("0.2.0");
        let change = m.update_to(id, Target::Ref("main".into())).unwrap();
        assert!(change.changed, "branch moved upstream");
        assert_eq!(change.to.commit, next);
        // HEAD means upstream HEAD, not the detached checkout.
        let head = ws.author.commit("0.3.0");
        assert_eq!(
            m.update_to(id, Target::Ref("HEAD".into()))
                .unwrap()
                .to
                .commit,
            head
        );
        ws.refuses("invalid_argument", || {
            m.update_to(id, Target::Ref("refs/heads/main".into()))
        });
        ws.refuses("invalid_argument", || {
            m.update_to(id, Target::Ref("no-such-ref".into()))
        });
    }

    // ---- enable / forget / uninstall ------------------------------------------------

    #[test]
    fn enable_and_disable_change_only_the_flag() {
        let (ws, m) = Ws::installed("dan/demo1");
        let before = serde_json::to_value(&m.state().unwrap().packages["dan/demo1"]).unwrap();
        let payload = tree_hash(&ws.root.join("repos"));
        m.disable("dan/demo1").unwrap();
        m.disable("dan/demo1").unwrap();
        let mut after = serde_json::to_value(&m.state().unwrap().packages["dan/demo1"]).unwrap();
        assert_eq!(after["enabled"], false);
        after["enabled"] = true.into();
        assert_eq!(after, before);
        m.set_enabled("dan/demo1", true).unwrap();
        assert!(m.list().unwrap()[0].enabled);
        assert_eq!(tree_hash(&ws.root.join("repos")), payload);
        ws.no_leftovers();
    }

    #[test]
    fn mutating_results_carry_the_dist_report() {
        let (ws, m) = Ws::installed("dan/demo1");
        let id = "dan/demo1";
        // A folder of no package is reported, never touched.
        fs::create_dir_all(ws.root.join("dist/custom")).unwrap();
        let other = ws.other_author("other", "dan/other");
        let installed = m.install(other.source()).unwrap();
        assert_eq!(installed.dist.deployed, ["dan/other"]);
        assert!(installed.dist.removed.is_empty() && installed.dist.skipped.is_empty());
        assert_eq!(installed.dist.unmanaged, ["dist/custom"]);
        let report = m.disable(id).unwrap();
        assert!(report.deployed.is_empty());
        assert_eq!(report.removed, [id]);
        assert_eq!(report.unmanaged, ["dist/custom"]);
        assert_eq!(m.enable(id).unwrap().deployed, [id]);
        ws.author.release("0.2.0");
        let updated = m.update(id).unwrap();
        assert_eq!(updated.dist.deployed, [id]);
        assert_eq!(updated.dist.unmanaged, ["dist/custom"]);
        assert_eq!(m.rollback(id).unwrap().dist.deployed, [id]);
        let forgotten = m.forget(id).unwrap();
        assert!(forgotten.clone_kept);
        assert_eq!(forgotten.dist.removed, [id]);
        assert_eq!(m.restore(id).unwrap().dist.deployed, [id]);
        // An active package whose clone and deployed folder are gone is skipped, and every
        // other mutating result says so.
        fs::remove_dir_all(ws.root.join("repos").join(clones::repo_key(other.source()))).unwrap();
        fs::remove_dir_all(ws.root.join("dist/dan/other")).unwrap();
        let report = m.disable(id).unwrap();
        assert_eq!(
            report.skipped,
            [SkippedDist {
                id: "dan/other".into(),
                reason: "clone missing".into(),
                clone_missing: true,
                link: None,
            }]
        );
        assert_eq!(m.enable(id).unwrap().skipped.len(), 1);
        let removed = m.uninstall(id, &[], false).unwrap();
        assert_eq!(removed.removed, [id]);
        assert_eq!(removed.skipped[0].id, "dan/other");
        ws.no_leftovers();
    }

    #[test]
    fn download_records_disabled_packages_and_dependencies() {
        let ws = Ws::new("dan/app");
        let lib = ws.other_author("lib", "dan/lib");
        let location = ws.dir.join("catalog").to_str().unwrap().to_owned();
        catalog::register(&location, lib.source(), "v0.1.0").unwrap();
        let mut app = make_manifest("dan/app", "0.2.0");
        app.dependencies.insert("dan/lib".into(), "^0.1".into());
        write_manifest(&ws.author.path, &app);
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "0.2.0"]);
        ws.author.tag("0.2.0");
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location),
                custom_ssh: false,
            },
        )
        .unwrap();
        let downloaded = m
            .download(ws.author.source(), None, InstallTarget::Release)
            .unwrap();
        assert_eq!(
            (downloaded.id.as_str(), downloaded.current.version.as_str()),
            ("dan/app", "0.2.0")
        );
        assert!(!downloaded.enabled && downloaded.mode == Mode::Release);
        let dependencies: Vec<_> = downloaded
            .dependencies
            .iter()
            .map(|d| (d.id.as_str(), d.current.version.as_str(), d.enabled))
            .collect();
        assert_eq!(dependencies, [("dan/lib", "0.1.0", false)]);
        assert!(downloaded.dist.deployed.is_empty() && downloaded.dist.removed.is_empty());
        let state = m.state().unwrap();
        for id in ["dan/app", "dan/lib"] {
            assert!(!state.packages[id].enabled, "{id}");
            assert!(state.packages[id].mode.is_release(), "{id}");
            assert!(
                ws.root.join("worktrees").join(id).join(".git").is_file(),
                "{id}"
            );
            assert!(!ws.root.join("dist").join(id).exists(), "{id}");
        }
        // Downloading again changes nothing.
        let again =
            ws.refuses_nothing(|| m.download(ws.author.source(), None, InstallTarget::Release));
        assert!(again.already_installed && !again.enabled);
        // enable needs the dependency active; it works without the catalog (offline).
        drop(m);
        let m = ws.open();
        let error = ws.refuses("dependency_conflict", || m.enable("dan/app"));
        assert!(
            error.to_string().contains("dan/lib is disabled; enable it"),
            "{error}"
        );
        assert_eq!(m.enable("dan/lib").unwrap().deployed, ["dan/lib"]);
        assert_eq!(m.enable("dan/app").unwrap().deployed, ["dan/app"]);
        assert!(ws.root.join("dist/dan/app/index.js").is_file());
        ws.no_leftovers();
    }

    #[test]
    fn enable_refuses_inactive_dependencies_unless_forced() {
        let ws = Ws::new("dan/app");
        let lib = ws.other_author("lib", "dan/lib");
        let mut app = make_manifest("dan/app", "0.2.0");
        app.dependencies.insert("dan/lib".into(), "^0.1".into());
        write_manifest(&ws.author.path, &app);
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "0.2.0"]);
        ws.author.tag("0.2.0");
        let m = ws.open();
        m.download(lib.source(), None, InstallTarget::Release)
            .unwrap();
        m.download(ws.author.source(), None, InstallTarget::Release)
            .unwrap();
        // Both disabled: enable app names the disabled dependency and changes nothing.
        let error = ws.refuses("dependency_conflict", || m.enable("dan/app"));
        let text = error.to_string();
        assert!(text.contains("dan/lib is disabled; enable it"), "{text}");
        assert!(text.contains("force"), "{text}");
        // Forced: app is enabled and deployed alone, and reported broken.
        assert_eq!(
            m.enable_with("dan/app", true).unwrap().deployed,
            ["dan/app"]
        );
        let state = m.state().unwrap();
        assert!(state.packages["dan/app"].enabled && !state.packages["dan/lib"].enabled);
        assert!(!ws.root.join("dist/dan/lib").exists());
        let listed = m.list().unwrap();
        let row = &listed
            .iter()
            .find(|p| p.id == "dan/app")
            .unwrap()
            .dependencies[0];
        assert!(!row.satisfied);
        assert_eq!(row.problem.as_deref(), Some("dan/lib is disabled"));
        // Enabling an enabled package changes nothing.
        ws.refuses_nothing(|| m.enable("dan/app"));
        // A missing dependency is named too.
        m.disable("dan/app").unwrap();
        m.uninstall("dan/lib", &[], true).unwrap();
        let error = ws.refuses("dependency_conflict", || m.enable("dan/app"));
        assert!(
            error.to_string().contains("dan/lib ^0.1: not installed"),
            "{error}"
        );
        ws.no_leftovers();
    }

    #[test]
    fn sync_dist_fails_with_not_deployed_after_syncing_the_others() {
        let (ws, m) = Ws::installed("dan/demo1");
        let other = ws.other_author("other", "dan/other");
        m.install(other.source()).unwrap();
        // dan/demo1 lost its deployed folder (repairable), dan/other its clone (not).
        fs::remove_dir_all(ws.dist()).unwrap();
        fs::remove_dir_all(ws.root.join("repos").join(clones::repo_key(other.source()))).unwrap();
        fs::remove_dir_all(ws.root.join("dist/dan/other")).unwrap();
        let plan = m.sync_dist_plan().unwrap();
        assert_eq!(plan.deployed, ["dan/demo1"]);
        assert_eq!(plan.skipped[0].id, "dan/other");
        let error = m.sync_dist().unwrap_err();
        assert_eq!(error.kind(), "not_deployed");
        assert_eq!(error.to_string(), "1 package(s) not deployed: dan/other");
        let Error::NotDeployed { sync, .. } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(sync.deployed, ["dan/demo1"]);
        assert_eq!(sync.skipped[0].reason, "clone missing");
        // What could be synced is committed.
        assert!(ws.dist().join("index.js").is_file());
        ws.no_leftovers();
        assert_eq!(m.sync_dist().unwrap_err().kind(), "not_deployed");
        // A recorded package whose clone is gone can be uninstalled (record and dist);
        // then there is nothing left to deploy.
        let plan = m.uninstall_plan("dan/other").unwrap();
        assert!(plan.registered && plan.lost_work.is_empty());
        m.uninstall("dan/other", &[], false).unwrap();
        assert!(!m.state().unwrap().packages.contains_key("dan/other"));
        assert_eq!(m.sync_dist().unwrap(), DistSync::default());
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
                ws.root
                    .join("repos")
                    .join(clones::repo_key(ws.author.source()))
                    .to_string_lossy()
                    .into_owned(),
                ws.dist().to_string_lossy().into_owned()
            ]
        );
        fs::remove_dir_all(ws.dist()).unwrap();
        m.forget("dan/demo1").unwrap();
        let plan = m.uninstall_plan("dan/demo1").unwrap();
        assert!(!plan.registered);
        assert_eq!(
            plan.deletes,
            [
                ws.repo().to_string_lossy().into_owned(),
                ws.root
                    .join("repos")
                    .join(clones::repo_key(ws.author.source()))
                    .to_string_lossy()
                    .into_owned()
            ]
        );
        m.uninstall("dan/demo1", &[], false).unwrap();
        assert!(!ws.repo().exists() && !ws.dist().exists());
        assert!(m.list().unwrap().is_empty());
        ws.no_leftovers();
    }

    #[test]
    fn uninstall_keeps_other_plugins() {
        let (ws, m) = Ws::installed("dan/demo1");
        let other = ws.other_author("other", "dan/other");
        m.install(other.source()).unwrap();
        let other_repo = tree_hash(&ws.root.join("repos").join(clones::repo_key(other.source())));
        m.uninstall("dan/demo1", &[], false).unwrap();
        assert_eq!(
            tree_hash(&ws.root.join("repos").join(clones::repo_key(other.source()))),
            other_repo
        );
        assert!(ws.root.join("dist/dan/other/index.js").is_file());
        assert_eq!(m.list().unwrap().len(), 1);
    }

    // ---- protected (verified) catalogs ----------------------------------------------

    /// `dan/demo1` 0.1.0 and 0.2.0 registered in a verified catalog, installed from it at
    /// 0.1.0; returns the catalog folder.
    fn protected_install(ws: &Ws) -> PathBuf {
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        ws.author.release("0.2.0");
        catalog::register(&location, ws.author.source(), "v0.2.0").unwrap();
        crate::test_support::verify_catalog(&catalog_dir);
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location),
                custom_ssh: false,
            },
        )
        .unwrap();
        m.install_from_catalog("dan/demo1", Some("0.1.0")).unwrap();
        catalog_dir
    }

    #[test]
    fn protected_packages_update_only_to_catalog_versions() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let catalog_dir = protected_install(&ws);
        let recorded = catalog::canonical_location(catalog_dir.to_str().unwrap()).unwrap();
        // The origin has 0.3.0, which the catalog does not list.
        ws.author.release("0.3.0");
        // No catalog configured: the recorded location is read.
        let m = ws.open();
        assert_eq!(
            m.state().unwrap().packages[id].catalog.as_deref(),
            Some(recorded.as_str())
        );
        let plan = m.update_all_plan().unwrap();
        assert_eq!(
            plan.updates
                .iter()
                .map(|c| c.to.version.as_str())
                .collect::<Vec<_>>(),
            ["0.2.0"]
        );
        let change = m.update(id).unwrap();
        assert_eq!(
            (change.changed, change.to.version.as_str()),
            (true, "0.2.0")
        );
        // Up to date with the catalog: the newer origin release is named, not installed.
        let change = ws.refuses_nothing(|| m.update(id));
        assert_eq!(
            (change.changed, change.unchanged_reason),
            (
                false,
                Some(UnchangedReason::NotInCatalog {
                    version: "0.3.0".into()
                })
            )
        );
        assert_eq!(
            UnchangedReason::NotInCatalog {
                version: "0.3.0".into()
            }
            .to_string(),
            "upstream 0.3.0 not in catalog"
        );
        let plan = m.update_all_plan().unwrap();
        assert!(plan.updates.is_empty());
        assert_eq!(
            plan.unchanged[0].reason,
            Some(UnchangedReason::NotInCatalog {
                version: "0.3.0".into()
            })
        );
        // A ref or branch target needs a detach.
        let error = ws.refuses("invalid_argument", || {
            m.update_to(id, Target::Ref("v0.3.0".into()))
        });
        assert!(error.to_string().contains("detach it"), "{error}");
        ws.refuses("invalid_argument", || m.update_to(id, Target::Dev));

        // A catalog release whose files do not match the tagged dist: refused, unchanged.
        let tampered = ws.author.release("0.4.0");
        catalog::register(catalog_dir.to_str().unwrap(), ws.author.source(), "v0.4.0").unwrap();
        let release_json = catalog_dir.join("namespaces/dan/demo1/versions/0.4.0/release.json");
        let mut value = crate::test_support::read_json(&release_json);
        value["files"]["index.js"]["sha256"] = serde_json::Value::String("0".repeat(64));
        crate::test_support::write_json(&release_json, &value);
        let error = ws.refuses("catalog", || m.update(id));
        assert!(
            error
                .to_string()
                .contains("catalog file hashes do not match git dist of dan/demo1@0.4.0"),
            "{error}"
        );
        assert!(
            error
                .to_string()
                .contains("differs from the one added to the catalog"),
            "{error}"
        );
        ws.refuses("catalog", || m.update_all());
        // A release commit the origin does not have.
        assert_ne!(tampered, "1".repeat(40));
        value["commit"] = serde_json::Value::String("1".repeat(40));
        crate::test_support::write_json(&release_json, &value);
        let error = ws.refuses("catalog", || m.update(id));
        assert!(
            error.to_string().contains("is not in the package's clone"),
            "{error}"
        );

        // Detach: the record loses `catalog` and the origin's newest release is taken.
        let change = m
            .update_with(id, Target::Recorded, UpdateOptions { detach: true })
            .unwrap();
        assert_eq!(
            (
                change.changed,
                change.to.version.as_str(),
                change.detached.as_deref()
            ),
            (true, "0.4.0", Some(recorded.as_str()))
        );
        assert!(m.state().unwrap().packages[id].catalog.is_none());
        let error = ws.refuses("invalid_argument", || {
            m.update_with(id, Target::Recorded, UpdateOptions { detach: true })
        });
        assert!(error.to_string().contains("not protected"), "{error}");
        // Unprotected now: a newer origin release is taken without the catalog.
        ws.author.release("0.5.0");
        assert_eq!(m.update(id).unwrap().to.version, "0.5.0");
        ws.no_leftovers();
    }

    #[test]
    fn detach_without_a_new_release_is_a_change() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        protected_install(&ws);
        let m = ws.open();
        m.update(id).unwrap();
        let change = m
            .update_with(id, Target::Recorded, UpdateOptions { detach: true })
            .unwrap();
        assert!(change.changed && change.detached.is_some());
        assert_eq!(change.from, change.to);
        assert!(m.state().unwrap().packages[id].catalog.is_none());
    }

    #[test]
    fn a_catalog_that_is_no_longer_verified_refuses_protected_updates() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let catalog_dir = protected_install(&ws);
        let path = catalog_dir.join("catalog.json");
        let mut value = crate::test_support::read_json(&path);
        value.as_object_mut().unwrap().remove("verified");
        crate::test_support::write_json(&path, &value);
        let m = ws.open();
        let error = ws.refuses("catalog", || m.update(id));
        assert!(error.to_string().contains("no longer verified"), "{error}");
        // update all keeps only this package where it is, with the reason.
        let plan = ws.refuses_nothing(|| m.update_all_plan());
        let reason = plan.unchanged[0].reason.clone();
        assert!(
            matches!(&reason, Some(UnchangedReason::CatalogUnavailable { reason })
                if reason.contains("no longer verified")),
            "{reason:?}"
        );
        assert!(
            reason
                .unwrap()
                .to_string()
                .starts_with("catalog unavailable: ")
        );
        // check names it on the row; the status follows the versions.
        let report = check::check(&ws.root, None).unwrap();
        assert_eq!(
            report.rows[0].problem.as_deref(),
            Some(check::CATALOG_NO_LONGER_VERIFIED)
        );
        assert_eq!(report.rows[0].status, check::CheckStatus::UpdateAvailable);
        // An unreachable catalog: the same for update all.
        fs::remove_dir_all(&catalog_dir).unwrap();
        let plan = m.update_all_plan().unwrap();
        assert!(matches!(
            plan.unchanged[0].reason,
            Some(UnchangedReason::CatalogUnavailable { .. })
        ));
    }

    /// `dan/demo1` 0.1.0 protected by a verified catalog that also lists `dan/lib`
    /// 0.1.0 and `dan/demo1` 0.2.0, which needs `dan/lib ^0.1`; returns the catalog.
    fn protected_with_new_dependency(ws: &Ws) -> PathBuf {
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        let lib = ws.other_author("lib", "dan/lib");
        catalog::register(&location, lib.source(), "v0.1.0").unwrap();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        crate::test_support::verify_catalog(&catalog_dir);
        Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location.clone()),
                custom_ssh: false,
            },
        )
        .unwrap()
        .install_from_catalog("dan/demo1", None)
        .unwrap();
        let mut app = make_manifest("dan/demo1", "0.2.0");
        app.dependencies.insert("dan/lib".into(), "^0.1".into());
        write_manifest(&ws.author.path, &app);
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "0.2.0"]);
        ws.author.tag("0.2.0");
        catalog::register(&location, ws.author.source(), "v0.2.0").unwrap();
        catalog_dir
    }

    #[test]
    fn new_dependencies_of_a_protected_package_come_from_its_catalog() {
        let recorded = |dir: &Path| catalog::canonical_location(dir.to_str().unwrap()).unwrap();
        // (a) update ID without a configured catalog.
        let ws = Ws::new("dan/demo1");
        let catalog_dir = protected_with_new_dependency(&ws);
        let m = ws.open();
        let change = m.update("dan/demo1").unwrap();
        assert_eq!(change.to.version, "0.2.0");
        let state = m.state().unwrap();
        assert_eq!(
            state.packages["dan/lib"].catalog,
            Some(recorded(&catalog_dir))
        );
        drop(m);
        // (b) update all with an unverified catalog configured that lacks `dan/lib`.
        let ws = Ws::new("dan/demo1");
        let catalog_dir = protected_with_new_dependency(&ws);
        let other = ws.dir.join("other-catalog");
        let stranger = ws.other_author("stranger", "dan/stranger");
        catalog::register(other.to_str().unwrap(), stranger.source(), "v0.1.0").unwrap();
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(other.to_str().unwrap().into()),
                custom_ssh: false,
            },
        )
        .unwrap();
        let plan = m.update_all().unwrap();
        assert_eq!(plan.installs[0].id, "dan/lib");
        let state = m.state().unwrap();
        assert_eq!(state.packages["dan/demo1"].current.version, "0.2.0");
        assert_eq!(
            state.packages["dan/lib"].catalog,
            Some(recorded(&catalog_dir))
        );
        // update ID the same way, from a fresh workspace with the unverified catalog set.
        drop(m);
        let ws = Ws::new("dan/demo1");
        let catalog_dir = protected_with_new_dependency(&ws);
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(other.to_str().unwrap().into()),
                custom_ssh: false,
            },
        )
        .unwrap();
        m.update("dan/demo1").unwrap();
        assert_eq!(
            m.state().unwrap().packages["dan/lib"].catalog,
            Some(recorded(&catalog_dir))
        );
    }

    #[test]
    fn install_from_a_verified_catalog_protects_a_direct_install_of_its_release() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        crate::test_support::verify_catalog(&catalog_dir);
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location.clone()),
                custom_ssh: false,
            },
        )
        .unwrap();
        // Downloaded directly, then installed from the catalog: enabled and protected.
        m.download(ws.author.source(), None, InstallTarget::Release)
            .unwrap();
        assert!(m.state().unwrap().packages[id].catalog.is_none());
        let result = m.install_from_catalog(id, None).unwrap();
        assert!(result.enabled && !result.already_installed);
        assert!(result.warnings[0].contains("is now protected by catalog"));
        let state = m.state().unwrap();
        assert_eq!(
            state.packages[id].catalog,
            Some(catalog::canonical_location(&location).unwrap())
        );
        // An enabled direct install becomes protected the same way.
        m.update_with(id, Target::Recorded, UpdateOptions { detach: true })
            .unwrap();
        let again = m.install_from_catalog(id, None).unwrap();
        assert!(again.already_installed);
        assert!(m.state().unwrap().packages[id].catalog.is_some());

        // A direct install whose files differ from the catalog record: refused, unchanged.
        let ws = Ws::new(id);
        let catalog_dir = ws.dir.join("catalog");
        let location = catalog_dir.to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        crate::test_support::verify_catalog(&catalog_dir);
        // The tag moves to another commit of the same version (the old one stays reachable).
        fixture_git(&ws.author.path, &["branch", "keep"]);
        fs::write(ws.author.path.join("dist/index.js"), "moved\n").unwrap();
        fixture_git(&ws.author.path, &["commit", "-q", "-am", "moved"]);
        fixture_git(&ws.author.path, &["tag", "-f", "v0.1.0"]);
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location.clone()),
                custom_ssh: false,
            },
        )
        .unwrap();
        m.download(ws.author.source(), None, InstallTarget::Release)
            .unwrap();
        let error = ws.refuses("catalog", || m.install_from_catalog(id, None));
        assert!(
            error
                .to_string()
                .contains("cannot be protected by that catalog"),
            "{error}"
        );
        assert!(m.state().unwrap().packages[id].catalog.is_none());
    }

    #[test]
    fn unverified_catalog_installs_follow_the_origin() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let location = ws.dir.join("catalog").to_str().unwrap().to_owned();
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location),
                custom_ssh: false,
            },
        )
        .unwrap();
        m.install_from_catalog(id, None).unwrap();
        assert!(m.state().unwrap().packages[id].catalog.is_none());
        ws.author.release("0.2.0");
        assert_eq!(m.update(id).unwrap().to.version, "0.2.0");
    }

    #[test]
    fn check_compares_protected_packages_with_their_catalog() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let catalog_dir = protected_install(&ws);
        let location = catalog_dir.to_str().unwrap();
        let m = ws.open();
        m.update(id).unwrap();
        ws.author.release("0.3.0");
        for given in [None, Some(location)] {
            let report = check::check(&ws.root, given).unwrap();
            let row = &report.rows[0];
            assert_eq!(
                (
                    row.status,
                    row.source,
                    row.latest.as_deref(),
                    row.origin_latest.as_deref(),
                    row.update_available
                ),
                (
                    check::CheckStatus::UpToDate,
                    check::CheckSource::Catalog,
                    Some("0.2.0"),
                    None,
                    false
                ),
                "{given:?}"
            );
            assert_eq!(row.message(), "up to date");
            assert_eq!(report.catalog_verified, given.is_some());
        }
        // An unverified catalog given: reported as such.
        let other = ws.dir.join("other-catalog");
        catalog::register(other.to_str().unwrap(), ws.author.source(), "v0.3.0").unwrap();
        let report = check::check(&ws.root, Some(other.to_str().unwrap())).unwrap();
        assert!(!report.catalog_verified);
        assert_eq!(report.rows[0].status, check::CheckStatus::OtherCatalog);
    }

    // ---- catalog-backed operations --------------------------------------------------

    #[test]
    fn catalog_operations_need_a_configured_catalog() {
        let (ws, m) = Ws::installed("dan/demo1");
        // check needs no catalog: it reads the origin.
        let rows = m.check().unwrap();
        assert_eq!(rows[0].source, check::CheckSource::Origin);
        let error = ws.refuses("invalid_argument", || m.browse_catalog());
        assert_eq!(error.to_string(), "no catalog configured");
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
        catalog::register(&location, ws.author.source(), "v0.1.0").unwrap();
        ws.author.release("0.2.0");
        catalog::register(&location, ws.author.source(), "v0.2.0").unwrap();
        let m = Manager::open_with(
            &ws.root,
            Options {
                catalog: Some(location.clone()),
                custom_ssh: false,
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
        assert!(!m.browse_catalog().unwrap().items.is_empty());

        let installed = m.install_from_catalog("dan/demo1", Some("0.1.0")).unwrap();
        assert_eq!(installed.current.version, "0.1.0");
        let rows = m.check().unwrap();
        assert_eq!(rows[0].status, check::CheckStatus::UpdateAvailable);
        assert_eq!(rows[0].latest.as_deref(), Some("0.2.0"));
        m.update("dan/demo1").unwrap();
        assert_eq!(m.check().unwrap()[0].status, check::CheckStatus::UpToDate);
        ws.refuses("catalog", || m.install_from_catalog("dan/missing", None));
        // Installed from Git, offered by the catalog from the same source: already there.
        assert!(
            m.install_from_catalog("dan/demo1", None)
                .unwrap()
                .already_installed
        );
    }

    #[test]
    fn release_with_empty_files_is_still_verified() {
        let ws = Ws::new("dan/demo1");
        let m = ws.open();
        let release = catalog::Release {
            manifest: make_manifest("dan/demo1", "0.1.0"),
            source: ws.author.source().into(),
            commit: ws.author.head(),
            reference: "v0.1.0".into(),
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
        // Branch naming considers the recorded source, not local installation refs.
        fixture_git(&ws.author.path, &["branch", "fork"]);
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
                commit: base.clone(),
                source: Some(ws.author.source().into()),
            })
        );
        assert_eq!(fixture_git(&path, &["remote"]), "upstream");
        assert_eq!(
            fixture_git(&path, &["remote", "get-url", "upstream"]),
            ws.author.source()
        );
        assert_eq!(fixture_git(&path, &["branch", "--show-current"]), "fork-2");
        // `forkOf` in the manifest is the only fork provenance: no `pm.*` Git config.
        let config = std::process::Command::new("git")
            .args(["config", "--local", "--get-regexp", r"^pm\."])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(
            !config.status.success() && config.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&config.stdout)
        );
        assert_eq!(fixture_git(&path, &["status", "--porcelain"]), "");
        assert_eq!(
            fixture_git(&path, &["log", "-1", "--format=%s"]),
            "Fork vendor/demo1 as me/demo1"
        );
        // Installing the fork is independent of the original.
        let r = m
            .install_package(
                path.to_str().unwrap(),
                None,
                InstallTarget::Ref("HEAD".into()),
            )
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
            m.fork_in_namespace(id, &ws.repo().join("new/deeper"), "me")
        });
        fs::create_dir_all(&dest).unwrap();
        let error = ws.refuses("invalid_argument", || m.fork_in_namespace(id, &dest, "me"));
        assert!(
            error.to_string().starts_with("destination already exists"),
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
        fixture_git(&linked.path, &["tag", "-f", "v0.1.0"]);
        m.install(linked.source()).expect("dist/ has no symlink");
        m.disable("dan/demo1")
            .expect("other packages stay manageable");
        // The package itself stays updatable; its repository symlink is kept.
        linked.release("0.2.0");
        m.update("dan/linked").unwrap();
        assert!(
            fs::symlink_metadata(ws.root.join("worktrees/dan/linked").join("manifest-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        // A symlink inside dist/ is refused before anything is committed.
        let bad = ws.other_author("bad", "dan/bad");
        std::os::unix::fs::symlink("index.js", bad.path.join("dist/link.js")).unwrap();
        fixture_git(&bad.path, &["add", "--all"]);
        fixture_git(&bad.path, &["commit", "-q", "-m", "symlink in dist"]);
        fixture_git(&bad.path, &["tag", "-f", "v0.1.0"]);
        ws.refuses("unsupported", || m.install(bad.source()));
    }

    #[test]
    fn a_submodule_in_dist_deploys_an_empty_folder() {
        let (ws, m) = Ws::installed("dan/demo1");
        let bad = ws.other_author("bad", "dan/bad");
        let sha = transport::object_id(&bad.path, "HEAD", false).unwrap();
        fixture_git(
            &bad.path,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{sha},dist/vendor"),
            ],
        );
        fixture_git(&bad.path, &["commit", "-q", "-m", "submodule in dist"]);
        fixture_git(&bad.path, &["tag", "-f", "v0.1.0"]);
        let result = m.install(bad.source()).unwrap();
        let vendor = ws.root.join("dist/dan/bad/vendor");
        assert!(vendor.is_dir());
        assert_eq!(fs::read_dir(vendor).unwrap().count(), 0);
        assert_eq!(result.id, "dan/bad");
        assert!(
            result
                .dist
                .warnings
                .iter()
                .any(|warning| warning.contains("dist/vendor is a Git submodule"))
        );
        fs::remove_dir_all(ws.root.join("dist/dan/bad")).unwrap();
        let synced = m.sync_dist().unwrap();
        assert_eq!(synced.warnings, result.dist.warnings);
        assert!(ws.root.join("dist/dan/bad/vendor").is_dir());
        fs::remove_dir_all(ws.root.join("dist/dan/bad")).unwrap();
        fs::remove_dir_all(ws.dist()).unwrap();
        let state = m.state().unwrap();
        fs::remove_dir_all(
            ws.root
                .join(state.packages["dan/demo1"].clone_folder("dan/demo1")),
        )
        .unwrap();
        let Error::NotDeployed { sync, .. } = m.sync_dist().unwrap_err() else {
            panic!("missing clone must produce a partial reconciliation report");
        };
        assert_eq!(sync.warnings, result.dist.warnings);
        assert!(ws.root.join("dist/dan/bad/vendor").is_dir());
    }

    #[test]
    fn symlinked_namespace_directory_is_never_followed() {
        // Install: `repos` points outside; nothing may be written there.
        let ws = Ws::new("vendor/demo1");
        let m = ws.open();
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, ws.root.join("repos")).unwrap();
        ws.refuses("workspace", || m.install(ws.author.source()));
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

        // Uninstall/update of an installed package whose namespace was swapped for a
        // symlink: the external directory survives untouched.
        let (ws, m) = Ws::installed("dan/demo1");
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::rename(ws.root.join("repos"), outside.join("dan")).unwrap();
        std::os::unix::fs::symlink(outside.join("dan"), ws.root.join("repos")).unwrap();
        ws.refuses("workspace", || m.uninstall("dan/demo1", &[], true));
        assert!(
            outside
                .join("dan")
                .join(clones::repo_key(ws.author.source()))
                .join("objects")
                .is_dir()
        );
        ws.author.release("0.2.0");
        ws.refuses("workspace", || m.update("dan/demo1"));

        let (ws, m) = Ws::installed("dan/demo1");
        let outside = ws.dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::rename(ws.root.join("dist/dan"), outside.join("dan")).unwrap();
        std::os::unix::fs::symlink(outside.join("dan"), ws.root.join("dist/dan")).unwrap();
        ws.refuses("workspace", || m.uninstall("dan/demo1", &[], true));
        assert!(outside.join("dan/demo1").is_dir());
    }

    #[test]
    fn symlinked_worktree_parents_are_never_followed() {
        for relative in ["worktrees", "worktrees/dan"] {
            let (ws, m) = Ws::installed("dan/demo1");
            ws.author.release("0.2.0");
            m.update("dan/demo1").unwrap();
            let outside = ws.dir.join("outside");
            fs::rename(ws.root.join(relative), &outside).unwrap();
            std::os::unix::fs::symlink(&outside, ws.root.join(relative)).unwrap();
            let before = tree_hash(&outside);
            ws.refuses("workspace", || m.update_to("dan/demo1", Target::Release));
            ws.refuses("workspace", || m.rollback("dan/demo1"));
            ws.refuses("workspace", || m.uninstall("dan/demo1", &[], true));
            assert_eq!(tree_hash(&outside), before);
        }
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
    fn rollback_of_a_fork_treats_fetched_upstream_tags_as_published() {
        let ws = Ws::new("me/hello");
        // The fork's repository holds a tag of its upstream on a commit no branch of the
        // fork contains (the upstream released after the fork point).
        let author = &ws.author.path;
        fixture_git(author, &["switch", "-q", "--detach"]);
        write_manifest(author, &make_manifest("acme/hello", "0.0.1"));
        fixture_git(author, &["commit", "-qam", "upstream 0.0.1"]);
        fixture_git(author, &["tag", "v0.0.1"]);
        fixture_git(author, &["switch", "-q", "main"]);
        let m = ws.open();
        m.install(ws.author.source()).unwrap();
        ws.author.release("0.2.0");
        m.update("me/hello").unwrap();
        // Offline, as published as it was for the update.
        let change = m.rollback("me/hello").unwrap();
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("0.2.0", "0.1.0")
        );
        // A tag made by hand is still local work, for both.
        let repo = ws.repo();
        fixture_git(&repo, &["switch", "-q", "--detach"]);
        fs::write(repo.join("extra.txt"), "x\n").unwrap();
        fixture_git(&repo, &["add", "extra.txt"]);
        fixture_git(&repo, &["commit", "-q", "-m", "tag only"]);
        fixture_git(&repo, &["tag", "local-only"]);
        fixture_git(&repo, &["switch", "-q", "--detach", "HEAD~1"]);
        for error in [
            ws.refuses("local_changes", || m.rollback("me/hello")),
            ws.refuses("local_changes", || m.update("me/hello")),
        ] {
            let Error::LocalChanges { items, .. } = error else {
                unreachable!()
            };
            assert_eq!(
                items,
                [LostWork::UnpublishedTag {
                    name: "local-only".into()
                }]
            );
        }
    }

    #[test]
    fn rollback_to_a_pruned_commit_names_it() {
        let (ws, m) = Ws::installed("dan/demo1");
        let first = ws.author.head();
        // The origin rewrites its history: 0.1.0 is on no branch and its tag is gone.
        let author = &ws.author.path;
        fixture_git(author, &["switch", "-q", "--orphan", "rewritten"]);
        fs::create_dir_all(author.join("dist")).unwrap();
        ws.author.release("0.2.0");
        fixture_git(author, &["branch", "-q", "-M", "main"]);
        fixture_git(author, &["tag", "-d", "v0.1.0"]);
        m.update("dan/demo1").unwrap();
        let repo = ws.repo();
        for branch in fixture_git(
            &repo,
            &["for-each-ref", "--format=%(refname)", "refs/heads/"],
        )
        .lines()
        {
            fixture_git(&repo, &["update-ref", "-d", branch]);
        }
        fixture_git(
            &repo,
            &["update-ref", "-d", "refs/gitpm/dan/demo1/previous"],
        );
        fixture_git(&repo, &["reflog", "expire", "--expire=now", "--all"]);
        fixture_git(&repo, &["gc", "-q", "--prune=now"]);
        let error = ws.refuses("no_previous_version", || m.rollback("dan/demo1"));
        assert_eq!(
            error.to_string(),
            format!("previous revision {first} of dan/demo1 is no longer available")
        );
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
        /// `tilr/store` and `tilr/theme` at 1.0.0, sharing one repository release tag.
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
            mono.tag("1.0.0");
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
        fn tag(&self, version: &str) {
            fixture_git(&self.repo, &["tag", &format!("v{version}")]);
        }
        fn release(&self, name: &str, version: &str) -> String {
            self.write(name, version, &[]);
            for entry in fs::read_dir(self.repo.join("packages")).unwrap() {
                let path = entry.unwrap().path().join("plugin.json");
                if path.is_file() {
                    let mut manifest: serde_json::Value =
                        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    manifest["version"] = version.into();
                    fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
                }
            }
            let commit = self.commit(&format!("Release {version}"));
            self.tag(version);
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
            .install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        assert_eq!(store.current.version, "1.0.0");
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        assert_eq!(mono.dist("store"), "store 1.0.0");
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
        let clones = mono.clones();
        assert_eq!(clones.len(), 1, "{clones:?}");
        assert!(clones[0].starts_with("mono-"), "{clones:?}");
        assert!(!mono.root.join("packages/tilr").exists());
        let state = m.state().unwrap();
        let p = &state.packages["tilr/store"];
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
            m.install_package(mono.source(), Some("tilr/missing"), InstallTarget::Release)
        });
        // A repository release tag requires a separate package selector.
        m.install_package(
            mono.source(),
            Some("tilr/theme"),
            InstallTarget::Ref("v1.0.0".into()),
        )
        .unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
        // A root repository checks a given ID.
        let ws = Ws::new("dan/demo1");
        let other = ws.open();
        ws.refuses("invalid_argument", || {
            other.install_package(
                ws.author.source(),
                Some("dan/other"),
                InstallTarget::Release,
            )
        });
        other
            .install_package(
                ws.author.source(),
                Some("dan/demo1"),
                InstallTarget::Release,
            )
            .unwrap();
    }

    #[test]
    fn packages_release_together_and_update_independently() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
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
        // Its sibling stays at its installed revision until explicitly updated.
        assert_eq!(
            m.state().unwrap().packages["tilr/theme"].current.version,
            "1.0.0"
        );
        assert!(m.update("tilr/theme").unwrap().changed);
        assert_eq!(
            m.state().unwrap().packages["tilr/theme"].current.version,
            "1.1.0"
        );
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

        // --dev follows the default branch for one package.
        mono.write("theme", "2.1.0", &[]);
        let head = mono.commit("theme work");
        let dev = m.update_to("tilr/theme", Target::Dev).unwrap();
        assert_eq!(dev.to.commit, head);
        assert_eq!(mono.dist("theme"), "theme 2.1.0");
        assert_eq!(mono.dist("store"), "store 1.2.0");
        // HEAD of the shared clone is now theme's untagged revision: store still updates.
        mono.release("store", "2.2.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 2.2.0");
    }

    #[test]
    fn uninstalling_one_package_keeps_the_shared_clone() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert_eq!(plan.deletes.len(), 2, "{:?}", plan.deletes);
        assert!(
            plan.deletes
                .iter()
                .any(|path| path.ends_with("dist/tilr/store"))
        );
        m.uninstall("tilr/store", &[], false).unwrap();
        assert!(!mono.root.join("dist/tilr/store").exists());
        assert_eq!(mono.clones().len(), 1);
        mono.release("theme", "1.1.0");
        m.update("tilr/theme").unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.1.0");
        // Reinstall reuses the clone.
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        assert_eq!(mono.clones().len(), 1);
        m.uninstall("tilr/store", &[], false).unwrap();
        // The last package takes the clone with it; local work there is guarded.
        let clone = mono.root.join("worktrees/tilr/theme");
        fs::write(clone.join("scratch.txt"), "mine").unwrap();
        mono.refuses("local_changes", || m.update("tilr/theme"));
        let plan = m.uninstall_plan("tilr/theme").unwrap();
        assert_eq!(plan.deletes.len(), 3);
        assert!(!plan.lost_work.is_empty());
        mono.refuses("local_changes", || m.uninstall("tilr/theme", &[], false));
        m.uninstall(
            "tilr/theme",
            &m.uninstall_plan("tilr/theme").unwrap().lost_work,
            false,
        )
        .unwrap();
        assert!(mono.clones().is_empty());
        assert!(m.state().unwrap().packages.is_empty());
    }

    #[test]
    fn a_tag_without_its_package_is_ignored() {
        let mono = Mono::new();
        let m = mono.open();
        // v2.0.0 points at a commit where only theme exists.
        fs::remove_dir_all(mono.repo.join("packages/store")).unwrap();
        mono.write("theme", "2.0.0", &[]);
        mono.commit("drop store");
        mono.tag("2.0.0");
        let installed = m
            .install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        assert_eq!(installed.current.version, "1.0.0");
        assert!(installed.skipped.is_empty());
        let error = mono.refuses("invalid_argument", || {
            m.install_package(
                mono.source(),
                Some("tilr/store"),
                InstallTarget::Ref("v2.0.0".into()),
            )
        });
        assert!(
            error.to_string().contains("not in this repository"),
            "{error}"
        );
        // Legacy package tags are ignored without a warning.
        fixture_git(&mono.repo, &["tag", "tilr/theme@3.0.0"]);
        let installed = m
            .install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        assert_eq!(installed.current.version, "2.0.0");
    }

    #[test]
    fn plain_fork_preserves_workspace_until_explicit_install_and_replace() {
        let mono = Mono::new();
        for (name, id) in [("store", "demo/a"), ("theme", "demo/b")] {
            write_manifest(
                &mono.repo.join(format!("packages/{name}")),
                &make_manifest(id, "0.2.0"),
            );
        }
        mono.commit("Independent A and B");
        for tag in ["demo/a@0.2.0", "demo/b@0.2.0", "v0.2.0"] {
            fixture_git(&mono.repo, &["tag", tag]);
        }
        let m = mono.open();
        for id in ["demo/a", "demo/b"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        let state_path = mono.root.join("state.json");
        let state_before = fs::read(&state_path).unwrap();
        let before = m.state().unwrap();
        let a_dist = fs::read(mono.root.join("dist/demo/a/index.js")).unwrap();
        let b_dist = fs::read(mono.root.join("dist/demo/b/index.js")).unwrap();
        let tags = fixture_git(&mono.repo, &["show-ref", "--tags"]);
        let destination = mono.dir.join("my-fork");
        m.fork_in_namespace("demo/a", &destination, "my").unwrap();
        assert_eq!(fixture_git(&destination, &["show-ref", "--tags"]), tags);
        assert_eq!(fs::read(&state_path).unwrap(), state_before);
        assert_eq!(
            fs::read(mono.root.join("dist/demo/a/index.js")).unwrap(),
            a_dist
        );
        assert_eq!(
            fs::read(mono.root.join("dist/demo/b/index.js")).unwrap(),
            b_dist
        );
        assert!(!mono.root.join("dist/my").exists());
        for (name, id, original) in [("store", "my/a", "demo/a"), ("theme", "my/b", "demo/b")] {
            let manifest: Manifest = serde_json::from_slice(
                &fs::read(destination.join(format!("packages/{name}/plugin.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest.id, id);
            assert_eq!(manifest.fork_of.unwrap().id, original);
            assert_eq!(manifest.replaces.unwrap().id, original);
        }
        crate::scaffold::commit_package(&destination, "0.3.0", Some("my/a")).unwrap();
        let head = fixture_git(&destination, &["rev-parse", "HEAD"]);
        let error =
            crate::scaffold::commit_package(&destination, "0.3.0", Some("my/a")).unwrap_err();
        assert_eq!(error.kind(), "version_conflict");
        assert!(error.to_string().contains("tag already exists"));
        assert_eq!(fixture_git(&destination, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(&state_path).unwrap(), state_before);
        m.install_package(
            destination.to_str().unwrap(),
            Some("my/a"),
            InstallTarget::Release,
        )
        .unwrap();
        assert!(m.state().unwrap().replacements.is_empty());
        m.replace("demo/a", "my/a").unwrap();
        let after = m.state().unwrap();
        assert_eq!(
            serde_json::to_value(&after.packages["demo/b"]).unwrap(),
            serde_json::to_value(&before.packages["demo/b"]).unwrap()
        );
        assert_eq!(after.replacements.len(), 1);
        assert_eq!(after.replacements["demo/a"], "my/a");
        assert!(!after.packages.contains_key("my/b"));
        assert_eq!(
            fs::read(mono.root.join("dist/demo/b/index.js")).unwrap(),
            b_dist
        );
        assert_eq!(
            fs::read(mono.root.join("dist/my/a/index.js")).unwrap(),
            a_dist
        );
        assert!(!mono.root.join("dist/demo/a").exists());
        assert!(!mono.root.join("dist/my/b").exists());
    }

    #[test]
    fn a_fork_of_several_packages_starts_each_from_its_installed_revision() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        let old = fixture_git(&mono.repo, &["rev-parse", "HEAD"]);
        let mut store_manifest = package_manifest(&mono.repo, Some("packages/store")).unwrap();
        store_manifest.version = "1.1.0".into();
        write_manifest(&mono.repo.join("packages/store"), &store_manifest);
        mono.write("theme", "1.1.0", &[("tilr/store", "^1.0")]);
        mono.commit("theme 1.1.0");
        mono.tag("1.1.0");
        mono.release("store", "1.2.0");
        m.install_package(
            mono.source(),
            Some("tilr/theme"),
            InstallTarget::Ref("v1.1.0".into()),
        )
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
        // Dirty existing targets are also refused without touching their files.
        let single = mono.dir.join("forks/single");
        m.fork_packages(&["tilr/store"], &single, "s").unwrap();
        let before = fixture_git(&single, &["rev-parse", "HEAD"]);
        fs::write(
            single.join(packages::CONFIG),
            "{\"packages\":[\"packages/*\"]}",
        )
        .unwrap();
        let error = m.fork_packages(&["tilr/theme"], &single, "s").unwrap_err();
        assert!(
            error.to_string().contains("destination already exists"),
            "{error}"
        );
        assert_eq!(
            package_manifest(&single, Some("packages/theme"))
                .unwrap()
                .id,
            "s/theme"
        );
        assert_eq!(fixture_git(&single, &["rev-parse", "HEAD"]), before);
        assert_eq!(fixture_git(&destination, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn install_records_the_ref_mode() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let first = ws.author.head();
        let release = ws.author.release("0.2.0");
        // A branch only the origin has: the fresh clone knows it as origin/feature.
        fixture_git(&ws.author.path, &["checkout", "-q", "-b", "feature"]);
        let feature = ws.author.commit("0.3.0");
        fixture_git(&ws.author.path, &["checkout", "-q", "main"]);
        let main = ws.author.commit("0.4.0");
        let m = ws.open();
        let cases = [
            (InstallTarget::Release, &release, Mode::Release, None),
            (
                InstallTarget::Ref("v0.1.0".into()),
                &first,
                Mode::Pinned("v0.1.0".into()),
                Some(serde_json::json!({"pinned": "v0.1.0"})),
            ),
            (
                InstallTarget::Ref(first[..12].into()),
                &first,
                Mode::Pinned(first.clone()),
                Some(serde_json::json!({"pinned": first})),
            ),
            (
                InstallTarget::Ref("feature".into()),
                &feature,
                Mode::Branch("feature".into()),
                Some(serde_json::json!({"branch": "feature"})),
            ),
            (
                InstallTarget::Dev,
                &main,
                Mode::Branch("main".into()),
                Some(serde_json::json!({"branch": "main"})),
            ),
        ];
        for (target, commit, mode, json) in cases {
            let installed = m
                .install_package(ws.author.source(), None, target.clone())
                .unwrap();
            assert_eq!(&installed.current.commit, commit, "{target:?}");
            assert_eq!(m.state().unwrap().packages[id].mode, mode, "{target:?}");
            assert_eq!(m.list().unwrap()[0].mode, mode, "{target:?}");
            let state: serde_json::Value =
                serde_json::from_str(&read(&ws.root.join("state.json"))).unwrap();
            assert_eq!(
                state["packages"][id].get("mode").cloned(),
                json,
                "{target:?}"
            );
            m.uninstall(id, &[], false).unwrap();
        }
        // HEAD is the origin's default branch, by its name.
        for head in ["HEAD", "refs/remotes/origin/HEAD"] {
            m.install_package(ws.author.source(), None, InstallTarget::Ref(head.into()))
                .unwrap();
            assert_eq!(
                m.state().unwrap().packages[id].mode,
                Mode::Branch("main".into()),
                "{head}"
            );
            m.uninstall(id, &[], false).unwrap();
        }
        // Only exact refs: no stale local branch, no revision expressions, no short hex.
        for bad in [
            "refs/heads/main",
            "HEAD~1",
            "main~1",
            "v0.2.0~1",
            "v0.2.0^",
            "refs/tags/v0.2.0~1",
            "refs/remotes/origin/main~1",
            &first[..6],
            &first.to_uppercase(),
        ] {
            let error = ws.refuses("invalid_argument", || {
                m.install_package(ws.author.source(), None, InstallTarget::Ref(bad.into()))
            });
            assert!(!error.to_string().is_empty(), "{bad}");
        }
    }

    #[test]
    fn update_follows_and_switches_the_recorded_mode() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_package(
            ws.author.source(),
            None,
            InstallTarget::Ref("v0.1.0".into()),
        )
        .unwrap();
        ws.author.release("0.2.0");
        // Pinned: unchanged with its reason, nothing written.
        let change = ws.refuses_nothing(|| m.update(id));
        assert!(!change.changed && change.to.version == "0.1.0");
        assert_eq!(
            change.unchanged_reason,
            Some(UnchangedReason::Pinned {
                reference: "v0.1.0".into()
            })
        );
        // Back to releases: the record has no mode.
        let change = m.update_to(id, Target::Release).unwrap();
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("0.1.0", "0.2.0")
        );
        let state: serde_json::Value =
            serde_json::from_str(&read(&ws.root.join("state.json"))).unwrap();
        assert!(state["packages"][id].get("mode").is_none(), "{state}");
        // Follow the default branch, then by the recorded mode.
        let head = ws.author.commit("0.3.0");
        let change = m.update_to(id, Target::Dev).unwrap();
        assert_eq!(change.to.commit, head);
        assert_eq!(
            m.state().unwrap().packages[id].mode,
            Mode::Branch("main".into())
        );
        let change = m.update(id).unwrap();
        assert!(!change.changed && change.unchanged_reason.is_none());
        // A tag pins again (an explicit ref may go back to an older version).
        let change = m.update_to(id, Target::Ref("v0.2.0".into())).unwrap();
        assert_eq!(change.to.version, "0.2.0");
        assert_eq!(
            m.state().unwrap().packages[id].mode,
            Mode::Pinned("v0.2.0".into())
        );
        // At the newest release already: --release records the mode without moving,
        // and a change of mode alone is a change.
        let change = m.update_to(id, Target::Release).unwrap();
        assert!(change.changed && change.from == change.to);
        assert_eq!(change.mode, Mode::Release);
        assert_eq!(m.state().unwrap().packages[id].mode, Mode::Release);
        // Rollback swaps revision and mode together.
        m.update_to(id, Target::Dev).unwrap();
        let state = m.state().unwrap();
        assert_eq!(
            state.packages[id].previous.as_ref().unwrap().mode,
            Mode::Release
        );
        let change = m.rollback(id).unwrap();
        assert_eq!(
            (change.to.version.as_str(), &change.mode),
            ("0.2.0", &Mode::Release)
        );
        let p = &m.state().unwrap().packages[id];
        assert_eq!(p.mode, Mode::Release);
        assert_eq!(
            p.previous.as_ref().unwrap().mode,
            Mode::Branch("main".into())
        );
        m.rollback(id).unwrap();
        assert_eq!(
            m.state().unwrap().packages[id].mode,
            Mode::Branch("main".into())
        );
        // Branch → --release (down to the newest release, 0.2.0) → rollback: back on main.
        let change = m.update_to(id, Target::Release).unwrap();
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("0.3.0", "0.2.0")
        );
        assert_eq!(m.state().unwrap().packages[id].mode, Mode::Release);
        m.rollback(id).unwrap();
        let p = &m.state().unwrap().packages[id];
        assert_eq!(
            (p.current.version.as_str(), &p.mode),
            ("0.3.0", &Mode::Branch("main".into()))
        );
        // A tag at the installed commit pins: a change of mode alone.
        m.update_to(id, Target::Release).unwrap();
        let change = m.update_to(id, Target::Ref("v0.2.0".into())).unwrap();
        assert!(change.changed && change.from.commit == change.to.commit);
        assert_eq!(change.mode, Mode::Pinned("v0.2.0".into()));
    }

    #[test]
    fn a_branch_gone_from_the_origin_is_reported_not_fatal() {
        let ws = Ws::new("dan/a");
        let other = ws.other_author("other", "dan/other");
        let m = ws.open();
        fixture_git(&ws.author.path, &["checkout", "-q", "-b", "feature"]);
        ws.author.commit("0.2.0");
        fixture_git(&ws.author.path, &["checkout", "-q", "main"]);
        m.install_package(
            ws.author.source(),
            None,
            InstallTarget::Ref("feature".into()),
        )
        .unwrap();
        m.install(other.source()).unwrap();
        fixture_git(&ws.author.path, &["branch", "-q", "-D", "feature"]);
        other.release("0.2.0");
        let error = ws.refuses("version_conflict", || m.update("dan/a"));
        assert_eq!(
            error.to_string(),
            "dan/a: the origin no longer has the branch feature it follows; automatic update refused"
        );
        let plan = m.update_all().unwrap();
        assert_eq!(plan.updates.len(), 1, "{plan:?}");
        assert_eq!(plan.updates[0].id, "dan/other");
        assert_eq!(
            plan.unchanged,
            [Unchanged {
                id: "dan/a".into(),
                version: "0.2.0".into(),
                reason: Some(UnchangedReason::BranchGone {
                    branch: "feature".into()
                }),
                warnings: Vec::new(),
            }]
        );
    }

    #[test]
    fn restore_in_a_shared_clone_ignores_another_branch_s_revision() {
        let mono = Mono::new();
        let m = mono.open();
        // store follows the branch side, theme the default branch main.
        fixture_git(&mono.repo, &["checkout", "-q", "-b", "side"]);
        mono.write("store", "1.1.0", &[]);
        mono.commit("store on side");
        fixture_git(&mono.repo, &["checkout", "-q", "main"]);
        let theme = m
            .install_package(mono.source(), Some("tilr/theme"), InstallTarget::Dev)
            .unwrap()
            .current;
        m.install_package(
            mono.source(),
            Some("tilr/store"),
            InstallTarget::Ref("side".into()),
        )
        .unwrap();
        m.forget("tilr/theme").unwrap();
        // Store has its own revision on side; theme comes back at its recorded commit.
        let store_head = head_of(&mono.root.join("worktrees/tilr/store"));
        assert_ne!(store_head, theme.commit);
        let restored = m.restore("tilr/theme").unwrap();
        assert_eq!(restored.current, theme);
        assert_eq!(
            m.state().unwrap().packages["tilr/theme"].mode,
            Mode::Branch("main".into())
        );
    }

    #[test]
    fn branch_mode_restore_checks_dependencies() {
        let id = "dan/demo1";
        let ws = Ws::new(id);
        let m = ws.open();
        m.install_package(ws.author.source(), None, InstallTarget::Dev)
            .unwrap();
        m.forget(id).unwrap();
        // The branch moves on to a revision that needs a package nobody installed.
        let mut next = make_manifest(id, "0.2.0");
        next.dependencies
            .insert("dan/missing".into(), "^1.0".into());
        write_manifest(&ws.author.path, &next);
        fixture_git(
            &ws.author.path,
            &["commit", "-q", "-am", "needs dan/missing"],
        );
        fixture_git(&ws.repo(), &["fetch", "-q", "origin"]);
        fixture_git(&ws.repo(), &["checkout", "-q", "--detach", "origin/main"]);
        let error = ws.refuses("local_changes", || m.restore(id));
        assert!(
            matches!(error, Error::LocalChanges { items, .. } if items.iter().any(|item| matches!(item, LostWork::HeadMoved { .. })))
        );
        assert!(m.state().unwrap().forgotten.contains_key(id));
    }

    #[test]
    fn update_all_follows_each_mode() {
        let ws = Ws::new("dan/a");
        let b = ws.other_author("b", "dan/b");
        let c = ws.other_author("c", "dan/c");
        let m = ws.open();
        m.install(ws.author.source()).unwrap();
        m.install_package(b.source(), None, InstallTarget::Ref("v0.1.0".into()))
            .unwrap();
        m.install_package(c.source(), None, InstallTarget::Dev)
            .unwrap();
        ws.author.release("0.2.0");
        b.release("0.2.0");
        // The branch moves without a release (and without a version bump).
        fs::write(c.path.join("dist/extra.js"), "extra").unwrap();
        fixture_git(&c.path, &["add", "--all"]);
        fixture_git(&c.path, &["commit", "-q", "-m", "same version"]);
        let c_head = c.head();
        let plan = m.update_all_plan().unwrap();
        let updated: Vec<_> = plan.updates.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(updated, ["dan/a", "dan/c"]);
        assert_eq!(
            plan.unchanged,
            [Unchanged {
                id: "dan/b".into(),
                version: "0.1.0".into(),
                reason: Some(UnchangedReason::Pinned {
                    reference: "v0.1.0".into()
                }),
                warnings: Vec::new(),
            }]
        );
        assert!(plan.kept_back.is_empty());
        m.update_all().unwrap();
        let state = m.state().unwrap();
        assert_eq!(state.packages["dan/a"].current.version, "0.2.0");
        assert_eq!(state.packages["dan/b"].current.version, "0.1.0");
        assert_eq!(state.packages["dan/c"].current.commit, c_head);
        assert_eq!(state.packages["dan/c"].mode, Mode::Branch("main".into()));
        // A rewritten branch keeps the package with a reason; the others stay up to date.
        fixture_git(&c.path, &["reset", "-q", "--hard", "HEAD~1"]);
        c.commit("0.3.0");
        let plan = m.update_all_plan().unwrap();
        assert!(plan.updates.is_empty(), "{:?}", plan.updates);
        let reasons: Vec<_> = plan
            .unchanged
            .iter()
            .map(|u| (u.id.as_str(), u.reason.clone()))
            .collect();
        assert_eq!(
            reasons,
            [
                ("dan/a", None),
                (
                    "dan/b",
                    Some(UnchangedReason::Pinned {
                        reference: "v0.1.0".into()
                    })
                ),
                (
                    "dan/c",
                    Some(UnchangedReason::Diverged {
                        branch: "main".into()
                    })
                ),
            ]
        );
    }

    #[test]
    fn restore_works_alike_for_shared_clones_in_each_mode() {
        let mono = Mono::new();
        let m = mono.open();
        let store = m
            .install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap()
            .current;
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Dev)
            .unwrap();
        // Release mode, disabled: theme's separate worktree can advance while store
        // retains its recorded revision.
        m.disable("tilr/store").unwrap();
        m.forget("tilr/store").unwrap();
        mono.write("theme", "1.1.0", &[]);
        let next = mono.commit("theme 1.1.0");
        m.update("tilr/theme").unwrap();
        assert_eq!(head_of(&mono.root.join("worktrees/tilr/theme")), next);
        let restored = m.restore("tilr/store").unwrap();
        assert_eq!(restored.current, store);
        assert!(restored.dist.deployed.is_empty());
        let state = m.state().unwrap();
        let p = &state.packages["tilr/store"];
        assert!(!p.enabled && p.mode == Mode::Release && p.current == store);
        // A published moved HEAD in the forgotten package is still local work.
        m.forget("tilr/theme").unwrap();
        mono.write("theme", "1.2.0", &[]);
        mono.commit("theme 1.2.0");
        let theme = mono.root.join("worktrees/tilr/theme");
        fixture_git(&theme, &["fetch", "-q", "origin"]);
        fixture_git(&theme, &["checkout", "-q", "--detach", "origin/main"]);
        mono.refuses("local_changes", || m.restore("tilr/theme"));
        assert_eq!(
            m.state().unwrap().forgotten["tilr/theme"].current.commit,
            next
        );
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
        // Theme and store have separate worktrees at different recorded revisions.
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        let store_commit = head_of(&mono.repo);
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        assert_eq!(
            head_of(&mono.root.join("worktrees/tilr/store")),
            store_commit
        );
        let destination = mono.dir.join("forks/store");
        let path = m
            .fork_in_namespace("tilr/store", &destination, "me")
            .unwrap();
        assert_eq!(fixture_git(&path, &["branch", "--show-current"]), "fork");
        assert_eq!(
            fixture_git(&path, &["rev-parse", "refs/heads/fork"]),
            head_of(&path)
        );
        assert_eq!(
            fixture_git(&path, &["rev-parse", "HEAD^"]),
            head_of(&mono.repo)
        );
        assert_eq!(
            package_manifest(&path, Some("packages/store"))
                .unwrap()
                .fork_of
                .unwrap()
                .commit,
            store_commit
        );
        assert_eq!(
            package_manifest(&path, Some("packages/theme")).unwrap().id,
            "me/theme"
        );
        let config: packages::Config =
            serde_json::from_slice(&fs::read(path.join(packages::CONFIG)).unwrap()).unwrap();
        assert_eq!(config.packages, ["packages/*"]);
        assert!(fixture_git(&path, &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn a_root_package_moved_into_a_folder_keeps_updating_and_forks() {
        // Root and nested releases keep the same source-keyed clone.
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
        let p = &state.packages["acme/x"];
        assert_eq!(p.path.as_deref(), Some("packages/x"));
        assert_eq!(
            p.clone.as_deref(),
            Some(clones::repo_key(repo.to_str().unwrap()).as_str())
        );
        assert!(!p.is_root_package());
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
        // forget/restore of the keyed clone finds the folder too.
        m.forget("acme/x").unwrap();
        let restored = m.restore("acme/x").unwrap();
        assert_eq!(restored.current.version, "0.3.0");
        assert_eq!(
            m.state().unwrap().packages["acme/x"].path.as_deref(),
            Some("packages/x")
        );
    }

    #[test]
    fn a_forgotten_shared_package_keeps_its_clone_until_uninstalled() {
        // B4
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        m.forget("tilr/store").unwrap();
        let state = m.state().unwrap();
        assert!(!state.packages.contains_key("tilr/store"));
        assert_eq!(
            state.forgotten["tilr/store"].path.as_deref(),
            Some("packages/store")
        );
        assert!(!mono.root.join("dist/tilr/store").exists());
        let error = mono.refuses("already_installed", || {
            m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
        });
        assert!(
            error
                .to_string()
                .contains("was forgotten and its clone is kept"),
            "{error}"
        );
        // The last registered user goes; the forgotten one still holds the clone.
        let plan = m.uninstall_plan("tilr/theme").unwrap();
        assert_eq!(plan.deletes.len(), 2, "{:?}", plan.deletes);
        m.uninstall("tilr/theme", &[], false).unwrap();
        assert_eq!(mono.clones().len(), 1);
        let restored = m.restore("tilr/store").unwrap();
        assert_eq!(restored.current.version, "1.0.0");
        assert_eq!(mono.dist("store"), "store 1.0.0");
        let state = m.state().unwrap();
        assert!(state.forgotten.is_empty());
        assert_eq!(
            state.packages["tilr/store"].clone.as_deref(),
            Some(mono.clones()[0].as_str())
        );
        mono.release("store", "1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.1.0");
        // Uninstalling a forgotten package removes the clone it was last on (forget
        // already removed its dist).
        m.forget("tilr/store").unwrap();
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert!(!plan.registered);
        assert_eq!(plan.deletes.len(), 2, "{:?}", plan.deletes);
        m.uninstall("tilr/store", &[], false).unwrap();
        assert!(mono.clones().is_empty());
        assert!(!mono.root.join("dist/tilr/store").exists());
        let state = m.state().unwrap();
        assert!(state.packages.is_empty() && state.forgotten.is_empty());
        mono.refuses("repository_not_found", || {
            m.uninstall("tilr/store", &[], false)
        });
    }

    #[test]
    fn removing_a_package_keeps_other_worktree_head_and_local_files() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        let store = mono.root.join("worktrees/tilr/store");
        let theme = mono.root.join("worktrees/tilr/theme");
        let recorded = head_of(&store);
        mono.write("theme", "1.1.0", &[]);
        let dev = mono.commit("theme work");
        m.update_to("tilr/theme", Target::Dev).unwrap();
        assert_eq!(head_of(&theme), dev);
        fs::write(store.join("scratch.txt"), "mine").unwrap();
        m.forget("tilr/theme").unwrap();
        assert_eq!(head_of(&theme), dev);
        m.restore("tilr/theme").unwrap();
        m.uninstall("tilr/theme", &[], false).unwrap();
        assert!(!theme.exists());
        assert_eq!(head_of(&store), recorded);
        assert_eq!(read(&store.join("scratch.txt")), "mine");
        fs::remove_file(store.join("scratch.txt")).unwrap();
        mono.release("store", "1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.1.0");
    }

    #[test]
    fn a_defective_sibling_does_not_block_a_package() {
        // B6: theme has no dist/ at store's 1.1.0 tag.
        let mono = Mono::new();
        let m = mono.open();
        fs::remove_dir_all(mono.repo.join("packages/theme/dist")).unwrap();
        mono.release("store", "1.1.0");
        let result = m
            .install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
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
        // The shared newest release is broken for theme; an explicit older ref remains usable.
        mono.refuses("version_conflict", || {
            m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
        });
        m.install_package(
            mono.source(),
            Some("tilr/theme"),
            InstallTarget::Ref("v1.0.0".into()),
        )
        .unwrap();
        assert_eq!(mono.dist("theme"), "theme 1.0.0");
    }

    #[test]
    fn a_package_can_move_to_another_folder_between_releases() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
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
        mono.tag("1.1.0");
        m.update("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store moved");
        assert_eq!(
            m.state().unwrap().packages["tilr/store"].path.as_deref(),
            Some("apps/store")
        );
        m.rollback("tilr/store").unwrap();
        assert_eq!(mono.dist("store"), "store 1.0.0");
        assert_eq!(
            m.state().unwrap().packages["tilr/store"].path.as_deref(),
            Some("packages/store")
        );
        m.update_all().unwrap();
        assert_eq!(mono.dist("store"), "store moved");
    }

    #[test]
    fn a_fork_ignores_local_edits_and_uses_each_installed_revision() {
        // B10: store is installed at the first commit, its folder changed since.
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        mono.write("store", "1.1.0", &[]);
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        let clone = mono.root.join("worktrees/tilr/store");
        fs::write(clone.join("packages/store/dist/index.js"), "store edit").unwrap();
        let destination = mono.dir.join("forks/both");
        m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
            .unwrap();
        assert_eq!(
            read(&destination.join("packages/store/dist/index.js")),
            "store 1.0.0"
        );
        assert_eq!(
            read(&clone.join("packages/store/dist/index.js")),
            "store edit"
        );
        // A missing installed revision is reported as such, not as diverged history.
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(mono.root.join("state.json")).unwrap()).unwrap();
        state["packages"]["tilr/store"]["current"]["commit"] = "1".repeat(40).into();
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
    fn a_submodule_outside_dist_is_preserved_at_its_source_revision() {
        let mono = Mono::new();
        // Use a real repository and a relative URL, then advance it after recording
        // the link: initialization must use the recorded commit, not its latest HEAD.
        let module = mono.dir.join("vendor");
        fs::create_dir_all(&module).unwrap();
        fixture_git(&module, &["init", "-q"]);
        fixture_git(&module, &["config", "user.name", "Test"]);
        fixture_git(&module, &["config", "user.email", "test@example.invalid"]);
        fs::write(module.join("source.txt"), "recorded").unwrap();
        fixture_git(&module, &["add", "."]);
        fixture_git(&module, &["commit", "-qm", "module"]);
        let sha = head_of(&module);
        fixture_git(
            &mono.repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "../vendor",
                "packages/theme/src/vendor",
            ],
        );
        mono.commit("source submodule");
        mono.tag("1.0.1");
        // Use this revision as theme's installed revision.
        let m = mono.open();
        m.install_package(
            mono.source(),
            Some("tilr/theme"),
            InstallTarget::Ref("HEAD".into()),
        )
        .unwrap();
        fs::write(module.join("source.txt"), "newer").unwrap();
        fixture_git(&module, &["commit", "-qam", "newer module"]);
        // A newer sibling revision removes the module. Theme must restore both the
        // gitlink and configuration from its older installed revision.
        fixture_git(&mono.repo, &["rm", "-f", "packages/theme/src/vendor"]);
        mono.release("store", "1.1.0");
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        let destination = mono.dir.join("forks/me");
        m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
            .unwrap();
        let link = fixture_git(
            &destination,
            &["ls-tree", "HEAD", "packages/theme/src/vendor"],
        );
        assert!(link.starts_with(&format!("160000 commit {sha}")), "{link}");
        fixture_git(
            &destination,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ],
        );
        assert_eq!(head_of(&destination.join("packages/theme/src/vendor")), sha);
        assert_eq!(
            read(&destination.join("packages/theme/src/vendor/source.txt")),
            "recorded"
        );
        assert_eq!(
            crate::model::manifest(&destination.join("packages/theme"))
                .unwrap()
                .id,
            "me/theme"
        );
        let before = snapshot(&destination);
        let error = m
            .fork_packages(&["tilr/store"], &destination, "me")
            .unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert_eq!(snapshot(&destination), before);
    }

    #[test]
    fn dependency_cycle_between_forks_can_be_copied_without_installing() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        mono.write("theme", "1.1.0", &[("tilr/store", "^1.0")]);
        mono.commit("theme needs store");
        mono.tag("1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        mono.write("store", "1.2.0", &[("tilr/theme", "^1.0")]);
        mono.write("theme", "1.2.0", &[("tilr/store", "^1.0")]);
        mono.commit("store needs theme");
        mono.tag("1.2.0");
        m.update("tilr/store").unwrap();
        let destination = mono.dir.join("forks/cycle");
        m.fork_packages(&["tilr/store", "tilr/theme"], &destination, "me")
            .unwrap();
    }

    #[test]
    fn a_workspace_of_another_state_format_is_refused_unchanged() {
        let (ws, m) = Ws::installed("dan/demo1");
        drop(m);
        let path = ws.root.join("state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        // gitpm 0.4/0.5: format 2, packages under `plugins`, clones under `plugins/`.
        let packages = state.as_object_mut().unwrap().remove("packages").unwrap();
        state["plugins"] = packages;
        for format in [None, Some(2), Some(4)] {
            match format {
                Some(n) => state["format"] = n.into(),
                None => drop(state.as_object_mut().unwrap().remove("format")),
            }
            fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
            let before = tree_hash(ws.dir.path());
            let error = reopen(&ws.root).err().unwrap();
            assert_eq!(error.kind(), "unsupported", "{format:?}");
            assert!(error.to_string().contains("only format 1"), "{error}");
            assert_eq!(
                tree_hash(ws.dir.path()),
                before,
                "nothing is rewritten or created"
            );
        }
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
    #[test]
    fn installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc() {
        let mono = Mono::new();
        let first = fixture_git(&mono.repo, &["rev-parse", "HEAD"]);
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        mono.release("theme", "1.1.0");
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        // theme follows the default branch to an untagged commit; 1.1.0 is its previous.
        mono.write("theme", "1.2.0", &[]);
        let dev = mono.commit("theme dev");
        m.update_to("tilr/theme", Target::Dev).unwrap();
        assert_eq!(
            m.state().unwrap().packages["tilr/theme"].current.commit,
            dev
        );
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
        fixture_git(&mono.repo, &["tag", "-d", "v1.1.0"]);
        mono.release("store", "1.1.0");
        // Updating store prunes and moves HEAD: only the keep refs hold theme's revisions.
        m.update("tilr/store").unwrap();
        gc(&clone);
        for commit in [
            &dev,
            &m.state().unwrap().packages["tilr/theme"]
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
        // A whole-repository fork also includes store: its rewritten history diverges
        // from theme's recorded revision, so it cannot form one fork branch.
        let destination = mono.dir.join("theme-fork");
        let error = m
            .fork_packages(&["tilr/theme"], &destination, "me")
            .unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(error.to_string().contains("on diverged histories"));
        assert!(!destination.exists());
        // Retaining store leaves theme as the sole installed revision to pin. The
        // fork still includes every package from theme's source revision.
        m.forget("tilr/store").unwrap();
        assert!(keep_refs(&clone).contains(&"refs/gitpm/tilr/store/current".to_owned()));
        let fork = m
            .fork_packages(&["tilr/theme"], &destination, "me")
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
        m.uninstall("tilr/store", &[], false).unwrap();
        assert_eq!(
            keep_refs(&clone),
            [
                "refs/gitpm/tilr/theme/current",
                "refs/gitpm/tilr/theme/previous"
            ]
        );
        // Upstream dropped theme's revisions: they are local work now.
        m.uninstall(
            "tilr/theme",
            &m.uninstall_plan("tilr/theme").unwrap().lost_work,
            false,
        )
        .unwrap();
        assert!(mono.clones().is_empty());
    }

    #[test]
    fn a_moved_tag_of_a_package_that_is_not_installed_is_taken_from_upstream() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        // theme 1.0.0 is retagged upstream (it was never installed here).
        fs::write(
            mono.repo.join("packages/theme/dist/index.js"),
            "theme retagged",
        )
        .unwrap();
        let moved = mono.commit("retag theme");
        fixture_git(&mono.repo, &["tag", "-f", "v1.0.0"]);
        let installed = m
            .install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        assert_eq!(installed.current.commit, moved);
        assert_eq!(mono.dist("theme"), "theme retagged");
        assert_eq!(mono.dist("store"), "store 1.0.0");
    }

    #[test]
    fn a_deleted_tag_in_a_shared_clone_is_reported_not_fatal() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        fixture_git(&mono.repo, &["tag", "-d", "v1.0.0"]);
        let retracted = ["installed release tilr/store@1.0.0 no longer exists on the origin"];
        let change = m.update("tilr/store").unwrap();
        assert!(!change.changed);
        assert_eq!(change.warnings, retracted);
        let plan = m.update_all().unwrap();
        assert!(plan.updates.is_empty());
        let store = plan
            .unchanged
            .iter()
            .find(|u| u.id == "tilr/store")
            .unwrap();
        assert_eq!(store.warnings, retracted);
        assert_eq!(mono.dist("store"), "store 1.0.0");
    }

    #[test]
    fn sibling_install_preserves_an_unpublished_colliding_tag() {
        let mono = Mono::new();
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        let checkout = mono.root.join("worktrees/tilr/store");
        fixture_git(&checkout, &["commit", "--allow-empty", "-m", "local only"]);
        let local = fixture_git(&checkout, &["rev-parse", "HEAD"]);
        fixture_git(&checkout, &["tag", "-f", "v1.0.0"]);
        let recorded = m.state().unwrap().packages["tilr/store"]
            .current
            .commit
            .clone();
        fixture_git(&checkout, &["checkout", "--detach", &recorded]);
        let state = fs::read(mono.root.join("state.json")).unwrap();
        assert_eq!(
            m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
                .unwrap_err()
                .kind(),
            "local_changes"
        );
        assert_eq!(
            fixture_git(&mono.clone_dir(), &["rev-parse", "refs/tags/v1.0.0"]),
            local
        );
        assert_eq!(fs::read(mono.root.join("state.json")).unwrap(), state);
    }

    #[test]
    fn origin_tags_are_read_from_the_origin_never_from_local_tags() {
        let mono = Mono::new();
        fixture_git(&mono.repo, &["tag", "extra"]);
        let m = mono.open();
        m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
            .unwrap();
        let clone = mono.clone_dir();
        assert!(resolve_ref(&clone, "refs/origin-tags/extra", false).is_ok());
        // A tag made by hand on a published commit, then the origin drops the commit
        // and the tag `extra`; installing a sibling fetches without pruning tags.
        fixture_git(&clone, &["tag", "mine"]);
        fixture_git(&mono.repo, &["tag", "-d", "extra"]);
        m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
            .unwrap();
        assert!(resolve_ref(&clone, "refs/origin-tags/extra", false).is_err());
        assert!(resolve_ref(&clone, "refs/origin-tags/mine", false).is_err());
        fixture_git(&mono.repo, &["switch", "-q", "--orphan", "rewritten"]);
        mono.write("store", "2.0.0", &[]);
        mono.write("theme", "2.0.0", &[]);
        mono.commit("rewritten");
        fixture_git(&mono.repo, &["branch", "-q", "-M", "main"]);
        fixture_git(
            &clone,
            &[
                "fetch",
                "-q",
                "--force",
                "origin",
                "+refs/heads/*:refs/remotes/origin/*",
                "--prune",
            ],
        );
        // The clone goes with its last package: its plan lists the hand-made tag.
        assert!(m.uninstall_plan("tilr/theme").unwrap().lost_work.is_empty());
        m.uninstall("tilr/theme", &[], false).unwrap();
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert!(
            plan.lost_work.contains(&LostWork::UnpublishedTag {
                name: "mine".into()
            }),
            "{:?}",
            plan.lost_work
        );
    }

    #[test]
    fn a_moved_tag_of_an_installed_package_does_not_block_a_sibling() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        let store = m.state().unwrap().packages["tilr/store"]
            .current
            .commit
            .clone();
        mono.write("dock", "1.0.0", &[]);
        mono.commit("dock");
        fs::write(
            mono.repo.join("packages/store/dist/index.js"),
            "store moved",
        )
        .unwrap();
        let moved = mono.commit("move store");
        fixture_git(&mono.repo, &["tag", "-f", "v1.0.0"]);
        m.install_package(mono.source(), Some("tilr/dock"), InstallTarget::Release)
            .unwrap();
        assert_eq!(m.update("tilr/theme").unwrap().to.version, "1.0.0");
        // The shared clone took the moved tag; tilr/store keeps its installed commit.
        assert_eq!(
            resolve_ref(&mono.clone_dir(), "v1.0.0", false).unwrap(),
            moved
        );
        assert_eq!(
            m.state().unwrap().packages["tilr/store"].current.commit,
            store
        );
        assert_eq!(mono.dist("store"), "store 1.0.0");
        let warning = format!(
            "release tag v1.0.0 now points to {}; the installed commit is kept",
            &moved[..12]
        );
        assert_eq!(m.update("tilr/store").unwrap().warnings, [warning.as_str()]);
        let plan = m.update_all().unwrap();
        let unchanged = plan
            .unchanged
            .iter()
            .find(|u| u.id == "tilr/store")
            .unwrap();
        assert_eq!(unchanged.warnings, [warning]);
    }

    #[test]
    fn a_forgotten_record_whose_clone_is_gone_can_be_uninstalled() {
        let mono = Mono::new();
        let m = mono.open();
        for id in ["tilr/store", "tilr/theme"] {
            m.install_package(mono.source(), Some(id), InstallTarget::Release)
                .unwrap();
        }
        m.forget("tilr/store").unwrap();
        m.uninstall("tilr/theme", &[], false).unwrap();
        fs::remove_dir_all(mono.clone_dir()).unwrap();
        // Like a restore without a clone: the clone is what restore needs.
        let error = mono.refuses("repository_not_found", || m.restore("tilr/store"));
        assert!(matches!(&error, Error::RepositoryNotFound { id } if id == "tilr/store"));
        let plan = m.uninstall_plan("tilr/store").unwrap();
        assert!(!plan.registered && plan.lost_work.is_empty());
        assert!(plan.deletes.is_empty(), "{:?}", plan.deletes);
        m.uninstall("tilr/store", &[], false).unwrap();
        let state = m.state().unwrap();
        assert!(state.forgotten.is_empty() && state.packages.is_empty());
        assert!(!mono.root.join("dist/tilr/store").exists());
    }

    /// A recorded package whose clone was deleted by hand: uninstall and forget drop the
    /// record and dist without confirmation, and forget keeps no `forgotten` record
    /// (clones with one or several package users alike).
    #[test]
    fn uninstall_and_forget_of_a_package_without_clone_drop_the_record() {
        let id = "dan/demo1";
        for forget in [false, true] {
            let (ws, m) = Ws::installed(id);
            fs::remove_dir_all(ws.repo()).unwrap();
            let plan = m.uninstall_plan(id).unwrap();
            assert!(plan.lost_work.is_empty() && plan.refusal().is_none());
            let dist = if forget {
                let forgotten = m.forget(id).unwrap();
                assert!(!forgotten.clone_kept);
                forgotten.dist
            } else {
                m.uninstall(id, &[], false).unwrap()
            };
            assert_eq!(dist.removed, [id]);
            assert!(dist.skipped.is_empty(), "{:?}", dist.skipped);
            let state = m.state().unwrap();
            assert!(!state.packages.contains_key(id) && !state.forgotten.contains_key(id));
            assert!(!ws.dist().exists());
            ws.refuses("repository_not_found", || m.restore(id));
            ws.no_leftovers();
        }
        // A package of a shared clone that is gone.
        for forget in [false, true] {
            let mono = Mono::new();
            let m = mono.open();
            m.install_package(mono.source(), Some("tilr/store"), InstallTarget::Release)
                .unwrap();
            m.install_package(mono.source(), Some("tilr/theme"), InstallTarget::Release)
                .unwrap();
            fs::remove_dir_all(mono.clone_dir()).unwrap();
            let dist = if forget {
                let forgotten = m.forget("tilr/store").unwrap();
                assert!(!forgotten.clone_kept);
                forgotten.dist
            } else {
                m.uninstall("tilr/store", &[], false).unwrap()
            };
            assert_eq!(dist.removed, ["tilr/store"]);
            let state = m.state().unwrap();
            assert!(!state.packages.contains_key("tilr/store"));
            assert!(!state.forgotten.contains_key("tilr/store"));
            assert!(state.packages.contains_key("tilr/theme"));
        }
    }

    /// Only a missing clone is flagged `clone_missing` (the CLI's advice to uninstall or
    /// forget depends on it); a missing installed commit is not.
    #[test]
    fn skipped_dists_say_structurally_whether_the_clone_is_missing() {
        let id = "dan/demo1";
        let (ws, m) = Ws::installed(id);
        let path = ws.root.join("state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        state["packages"][id]["current"]["commit"] = "0".repeat(40).into();
        fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
        fs::remove_dir_all(ws.dist()).unwrap();
        let plan = m.sync_dist_plan().unwrap();
        assert_eq!(
            plan.skipped,
            [SkippedDist {
                id: id.into(),
                reason: "installed commit missing from its clone".into(),
                clone_missing: false,
                link: None,
            }]
        );
        fs::remove_dir_all(ws.repo()).unwrap();
        let plan = m.sync_dist_plan().unwrap();
        assert!(plan.skipped[0].clone_missing);
    }
}
