//! UniFFI bindings for native apps (Swift/Kotlin). Consumer operations only: open a
//! workspace, browse a catalog, install/update/rollback/enable/disable/forget/restore,
//! plan and confirm uninstall, fork, replace a plugin by a fork and switch back,
//! check repository access.
//!
//! All calls block (Git/network); call them off the main thread.
use gitpm as pm;
use std::{collections::HashMap, sync::Arc};

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------------------------------
// Errors

/// Every variant carries the full human-readable `message`; the case is the stable kind.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PmError {
    #[error("{message}")]
    NotInstalled { id: String, message: String },
    #[error("{message}")]
    AlreadyInstalled { id: String, message: String },
    #[error("{message}")]
    LocalChanges {
        id: String,
        message: String,
        items: Vec<LostWork>,
    },
    #[error("{message}")]
    Locked { message: String },
    #[error("{message}")]
    InvalidId { message: String },
    #[error("{message}")]
    InvalidManifest { message: String },
    #[error("{message}")]
    InvalidVersion { message: String },
    #[error("{message}")]
    InvalidArgument { message: String },
    #[error("{message}")]
    VersionConflict { message: String },
    #[error("{message}")]
    NoPreviousVersion { id: String, message: String },
    #[error("{message}")]
    NoReleases { message: String },
    #[error("{message}")]
    RepositoryNotFound { id: String, message: String },
    #[error("{message}")]
    Workspace { message: String },
    #[error("{message}")]
    Catalog { message: String },
    #[error("{message}")]
    Git { message: String },
    #[error("{message}")]
    Http { message: String },
    #[error("{message}")]
    Auth { message: String },
    #[error("{message}")]
    Unsupported { message: String },
    /// Dependencies cannot be satisfied; `message` is the solver's explanation.
    #[error("{message}")]
    DependencyConflict { message: String },
    /// Other installed plugins depend on `id`; retry with `force` to proceed anyway.
    #[error("{message}")]
    HasDependents {
        id: String,
        message: String,
        dependents: Vec<String>,
    },
    /// `consumers` do not accept the new provider's version; retry with `confirm`.
    #[error("{message}")]
    IncompatibleReplacement {
        id: String,
        message: String,
        consumers: Vec<String>,
    },
    #[error("{message}")]
    InvalidData { message: String },
    #[error("{message}")]
    Io { message: String },
    /// A kind added to the core library later; `kind` is its stable identifier.
    #[error("{message}")]
    Other { kind: String, message: String },
}

impl From<pm::Error> for PmError {
    fn from(error: pm::Error) -> Self {
        let message = error.to_string();
        match error {
            pm::Error::NotInstalled { id } => PmError::NotInstalled { id, message },
            pm::Error::AlreadyInstalled { id, .. } => PmError::AlreadyInstalled { id, message },
            pm::Error::LocalChanges { id, items, .. } => PmError::LocalChanges {
                id,
                message,
                items: items.into_iter().map(Into::into).collect(),
            },
            pm::Error::Locked(_) => PmError::Locked { message },
            pm::Error::InvalidId(_) => PmError::InvalidId { message },
            pm::Error::InvalidManifest(_) => PmError::InvalidManifest { message },
            pm::Error::InvalidVersion(_) => PmError::InvalidVersion { message },
            pm::Error::InvalidArgument(_) => PmError::InvalidArgument { message },
            pm::Error::VersionConflict(_) => PmError::VersionConflict { message },
            pm::Error::NoPreviousVersion { id } => PmError::NoPreviousVersion { id, message },
            pm::Error::NoReleases(_) => PmError::NoReleases { message },
            pm::Error::RepositoryNotFound { id } => PmError::RepositoryNotFound { id, message },
            pm::Error::Workspace(_) => PmError::Workspace { message },
            pm::Error::Catalog(_) => PmError::Catalog { message },
            pm::Error::Git(_) => PmError::Git { message },
            pm::Error::Http(_) => PmError::Http { message },
            pm::Error::Auth(_) => PmError::Auth { message },
            pm::Error::Unsupported(_) => PmError::Unsupported { message },
            pm::Error::DependencyConflict(_) => PmError::DependencyConflict { message },
            pm::Error::HasDependents { id, dependents, .. } => PmError::HasDependents {
                id,
                message,
                dependents,
            },
            pm::Error::IncompatibleReplacement { id, consumers, .. } => {
                PmError::IncompatibleReplacement {
                    id,
                    message,
                    consumers,
                }
            }
            pm::Error::InvalidData(_) => PmError::InvalidData { message },
            pm::Error::Io(_) => PmError::Io { message },
            other => PmError::Other {
                kind: other.kind().into(),
                message,
            },
        }
    }
}

type Result<T> = std::result::Result<T, PmError>;

// ---------------------------------------------------------------------------------------
// Records

#[derive(Debug, Clone, uniffi::Record)]
pub struct Revision {
    pub version: String,
    pub commit: String,
}
impl From<pm::Revision> for Revision {
    fn from(r: pm::Revision) -> Self {
        Self {
            version: r.version,
            commit: r.commit,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkOf {
    pub id: String,
    pub version: String,
    pub commit: String,
}
impl From<pm::ForkOf> for ForkOf {
    fn from(f: pm::ForkOf) -> Self {
        Self {
            id: f.id,
            version: f.version,
            commit: f.commit,
        }
    }
}

/// The plugin can stand in for `id` at `version` (used only after an explicit replace).
#[derive(Debug, Clone, uniffi::Record)]
pub struct Replaces {
    pub id: String,
    pub version: String,
}
impl From<pm::Replaces> for Replaces {
    fn from(r: pm::Replaces) -> Self {
        Self {
            id: r.id,
            version: r.version,
        }
    }
}

/// One dependency of an installed plugin and the plugin that provides it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DependencyStatus {
    pub id: String,
    pub requirement: String,
    /// Installed provider (the replacement fork when `id` is replaced).
    pub provider: Option<String>,
    pub version: Option<String>,
    pub satisfied: bool,
    pub problem: Option<String>,
}
impl From<pm::deps::DependencyStatus> for DependencyStatus {
    fn from(d: pm::deps::DependencyStatus) -> Self {
        Self {
            id: d.id,
            requirement: d.requirement,
            provider: d.provider,
            version: d.version,
            satisfied: d.satisfied,
            problem: d.problem,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct InstalledPlugin {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    pub fork_of: Option<ForkOf>,
    pub replaces: Option<Replaces>,
    /// Catalog it was installed from; `None` for Git installs.
    pub catalog: Option<String>,
    /// Package folder in a multi-package repository; `None` for the repository root.
    pub path: Option<String>,
    /// Shared clone `repos/<clone>` of a multi-package repository; `None`: own clone.
    pub clone: Option<String>,
    /// Descriptive metadata from the installed `plugin.json` (stored, not interpreted).
    pub title: Option<String>,
    pub description: Option<String>,
    /// Services the package provides (`"provides": "theme"` reads as `["theme"]`).
    pub provides: Vec<String>,
    pub tags: Vec<String>,
    pub dependencies: Vec<DependencyStatus>,
    /// Original IDs this plugin currently provides for every consumer.
    pub replacing: Vec<String>,
    /// Installed plugins that depend on this one.
    pub dependents: Vec<String>,
}
impl From<pm::InstalledPlugin> for InstalledPlugin {
    fn from(p: pm::InstalledPlugin) -> Self {
        Self {
            id: p.id,
            source: p.source,
            current: p.current.into(),
            previous: p.previous.map(Into::into),
            enabled: p.enabled,
            fork_of: p.fork_of.map(Into::into),
            replaces: p.replaces.map(Into::into),
            catalog: p.catalog,
            path: p.path,
            clone: p.clone,
            title: p.title,
            description: p.description,
            provides: p.provides,
            tags: p.tags,
            dependencies: p.dependencies.into_iter().map(Into::into).collect(),
            replacing: p.replacing,
            dependents: p.dependents,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IncompatibleConsumer {
    pub id: String,
    pub requirement: String,
}
/// What replacing (or switching back) would change; see `PluginManager.replace`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReplacementPlan {
    pub original: String,
    pub provider: String,
    pub version: String,
    pub current: Option<String>,
    pub consumers: Vec<String>,
    /// Non-empty: ask the user, then call again with `confirm = true`.
    pub incompatible: Vec<IncompatibleConsumer>,
}
impl From<pm::ReplacementPlan> for ReplacementPlan {
    fn from(p: pm::ReplacementPlan) -> Self {
        Self {
            original: p.original,
            provider: p.provider,
            version: p.version,
            current: p.current,
            consumers: p.consumers,
            incompatible: p
                .incompatible
                .into_iter()
                .map(|c| IncompatibleConsumer {
                    id: c.id,
                    requirement: c.requirement,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct InstallResult {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub fork_of: Option<ForkOf>,
}
impl From<pm::InstallResult> for InstallResult {
    fn from(r: pm::InstallResult) -> Self {
        Self {
            id: r.id,
            source: r.source,
            current: r.current.into(),
            fork_of: r.fork_of.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RevisionChange {
    pub id: String,
    pub from: Revision,
    pub to: Revision,
    /// False when the plugin was already at the target revision.
    pub changed: bool,
}
impl From<pm::RevisionChange> for RevisionChange {
    fn from(c: pm::RevisionChange) -> Self {
        Self {
            id: c.id,
            from: c.from.into(),
            to: c.to.into(),
            changed: c.changed,
        }
    }
}

/// Serializable joint upgrade result, also used for a read-only preview.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdateAllPlan {
    pub updates: Vec<RevisionChange>,
    pub installs: Vec<InstallResult>,
    pub kept_back: Vec<HeldBack>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct HeldBack {
    pub id: String,
    pub selected: String,
    pub latest: String,
}
impl From<pm::UpdateAllPlan> for UpdateAllPlan {
    fn from(plan: pm::UpdateAllPlan) -> Self {
        Self {
            updates: plan.updates.into_iter().map(Into::into).collect(),
            installs: plan.installs.into_iter().map(Into::into).collect(),
            kept_back: plan
                .kept_back
                .into_iter()
                .map(|p| HeldBack {
                    id: p.id,
                    selected: p.selected,
                    latest: p.latest,
                })
                .collect(),
        }
    }
}

/// Local work that uninstall would discard.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum LostWork {
    Modified { path: String },
    Untracked { path: String },
    Ignored { path: String },
    HeadMoved { head: String, recorded: String },
    UnpublishedCommit { commit: String, summary: String },
    UnpublishedBranch { name: String, commits: u64 },
    UnpublishedTag { name: String },
    Stash { entry: String },
}
impl From<pm::LostWork> for LostWork {
    fn from(item: pm::LostWork) -> Self {
        match item {
            pm::LostWork::Modified { path } => LostWork::Modified { path },
            pm::LostWork::Untracked { path } => LostWork::Untracked { path },
            pm::LostWork::Ignored { path } => LostWork::Ignored { path },
            pm::LostWork::HeadMoved { head, recorded } => LostWork::HeadMoved { head, recorded },
            pm::LostWork::UnpublishedCommit { commit, summary } => {
                LostWork::UnpublishedCommit { commit, summary }
            }
            pm::LostWork::UnpublishedBranch { name, commits } => {
                LostWork::UnpublishedBranch { name, commits }
            }
            pm::LostWork::UnpublishedTag { name } => LostWork::UnpublishedTag { name },
            pm::LostWork::Stash { entry } => LostWork::Stash { entry },
        }
    }
}
impl From<LostWork> for pm::LostWork {
    fn from(item: LostWork) -> Self {
        match item {
            LostWork::Modified { path } => pm::LostWork::Modified { path },
            LostWork::Untracked { path } => pm::LostWork::Untracked { path },
            LostWork::Ignored { path } => pm::LostWork::Ignored { path },
            LostWork::HeadMoved { head, recorded } => pm::LostWork::HeadMoved { head, recorded },
            LostWork::UnpublishedCommit { commit, summary } => {
                pm::LostWork::UnpublishedCommit { commit, summary }
            }
            LostWork::UnpublishedBranch { name, commits } => {
                pm::LostWork::UnpublishedBranch { name, commits }
            }
            LostWork::UnpublishedTag { name } => pm::LostWork::UnpublishedTag { name },
            LostWork::Stash { entry } => pm::LostWork::Stash { entry },
        }
    }
}

/// One-line human description of a lost-work item.
#[uniffi::export]
pub fn describe_lost_work(item: LostWork) -> String {
    pm::LostWork::from(item).describe()
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct UninstallPlan {
    pub id: String,
    /// False for a forgotten repository that is still on disk.
    pub registered: bool,
    pub deletes: Vec<String>,
    /// Empty: uninstall without asking. Otherwise show it and call `uninstall(id, true)`
    /// only after the user confirmed.
    pub lost_work: Vec<LostWork>,
    /// Installed plugins that depend on it: uninstall refuses unless `force`.
    pub dependents: Vec<String>,
}
impl From<pm::UninstallPlan> for UninstallPlan {
    fn from(p: pm::UninstallPlan) -> Self {
        Self {
            id: p.id,
            registered: p.registered,
            deletes: p.deletes,
            lost_work: p.lost_work.into_iter().map(Into::into).collect(),
            dependents: p.dependents,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum CheckStatus {
    NotInCatalog,
    DifferentSource,
    InvalidCatalogData,
    NoVersions,
    UncomparableVersion,
    SameVersionDifferentCommit,
    UpdateAvailable,
    UpToDate,
    NewerThanCatalog,
    OtherCatalog,
}
impl From<pm::check::CheckStatus> for CheckStatus {
    fn from(s: pm::check::CheckStatus) -> Self {
        use pm::check::CheckStatus as S;
        match s {
            S::NotInCatalog => CheckStatus::NotInCatalog,
            S::DifferentSource => CheckStatus::DifferentSource,
            S::InvalidCatalogData => CheckStatus::InvalidCatalogData,
            S::NoVersions => CheckStatus::NoVersions,
            S::UncomparableVersion => CheckStatus::UncomparableVersion,
            S::SameVersionDifferentCommit => CheckStatus::SameVersionDifferentCommit,
            S::UpdateAvailable => CheckStatus::UpdateAvailable,
            S::UpToDate => CheckStatus::UpToDate,
            S::NewerThanCatalog => CheckStatus::NewerThanCatalog,
            S::OtherCatalog => CheckStatus::OtherCatalog,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CheckRow {
    pub id: String,
    pub installed: String,
    pub latest: Option<String>,
    pub enabled: bool,
    pub update_available: bool,
    pub status: CheckStatus,
    /// Human-readable status text.
    pub status_message: String,
    /// For a fork: where its original stands in the catalog.
    pub upstream: Option<Upstream>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct Upstream {
    pub id: String,
    /// Original version the fork started from.
    pub base: String,
    pub latest: Option<String>,
    /// The original has a newer release than the fork base.
    pub newer: bool,
}
impl From<pm::check::CheckRow> for CheckRow {
    fn from(r: pm::check::CheckRow) -> Self {
        Self {
            status_message: r.status.message().into(),
            id: r.id,
            installed: r.installed,
            latest: r.latest,
            enabled: r.enabled,
            update_available: r.update_available,
            status: r.status.into(),
            upstream: r.upstream.map(|u| Upstream {
                id: u.id,
                base: u.base,
                latest: u.latest,
                newer: u.newer,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum CatalogFormat {
    V1,
    V2,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct PluginSummary {
    pub id: String,
    pub source: String,
    pub versions: Vec<String>,
    pub latest: Option<String>,
    /// Descriptive metadata of the latest release.
    pub title: Option<String>,
    pub description: Option<String>,
    pub provides: Vec<String>,
    pub tags: Vec<String>,
}
#[derive(Debug, Clone, uniffi::Enum)]
pub enum OverviewItem {
    Directory { depth: u32, name: String },
    Empty { depth: u32 },
    Plugin { depth: u32, plugin: PluginSummary },
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct CatalogOverview {
    pub location: String,
    pub remote: bool,
    pub format: CatalogFormat,
    /// Depth-first rows of the catalog tree (flat; use `depth` for indentation).
    pub items: Vec<OverviewItem>,
    pub truncated: bool,
}
impl From<pm::catalog::CatalogOverview> for CatalogOverview {
    fn from(o: pm::catalog::CatalogOverview) -> Self {
        Self {
            location: o.location,
            remote: o.remote,
            format: match o.format {
                pm::catalog::CatalogFormat::V1 => CatalogFormat::V1,
                pm::catalog::CatalogFormat::V2 => CatalogFormat::V2,
            },
            items: o
                .items
                .into_iter()
                .map(|item| match item {
                    pm::catalog::OverviewItem::Directory { depth, name } => {
                        OverviewItem::Directory { depth, name }
                    }
                    pm::catalog::OverviewItem::Empty { depth } => OverviewItem::Empty { depth },
                    pm::catalog::OverviewItem::Plugin { depth, plugin } => OverviewItem::Plugin {
                        depth,
                        plugin: PluginSummary {
                            id: plugin.id,
                            source: plugin.source,
                            versions: plugin.versions,
                            latest: plugin.latest,
                            title: plugin.title,
                            description: plugin.description,
                            provides: plugin.provides,
                            tags: plugin.tags,
                        },
                    },
                })
                .collect(),
            truncated: o.truncated,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CatalogRelease {
    pub id: String,
    pub version: String,
    pub source: String,
    pub commit: String,
    pub reference: String,
    pub file_count: u64,
    pub fork_of: Option<ForkOf>,
    pub replaces: Option<Replaces>,
    /// Dependency ID → version requirement.
    pub dependencies: HashMap<String, String>,
    pub build_url: Option<String>,
    pub build_sha256: Option<String>,
    /// Package folder in a multi-package repository; `None` for the repository root.
    pub path: Option<String>,
    /// Descriptive metadata of the released manifest.
    pub title: Option<String>,
    pub description: Option<String>,
    pub provides: Vec<String>,
    pub tags: Vec<String>,
}
impl From<pm::catalog::Release> for CatalogRelease {
    fn from(r: pm::catalog::Release) -> Self {
        let metadata = r.metadata();
        Self {
            title: metadata.title,
            description: metadata.description,
            provides: metadata.provides,
            tags: metadata.tags,
            id: r.manifest.id,
            version: r.manifest.version,
            source: r.source,
            commit: r.commit,
            reference: r.reference,
            file_count: r.files.len() as u64,
            fork_of: r.manifest.fork_of.map(Into::into),
            replaces: r.manifest.replaces.map(Into::into),
            dependencies: r.manifest.dependencies.into_iter().collect(),
            build_url: r.build.as_ref().map(|b| b.url.clone()),
            build_sha256: r.build.map(|b| b.sha256),
            path: r.path,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AuthCheck {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkResult {
    pub id: String,
    pub path: String,
}

/// Step of a fork with replace that failed after the fork repository exists
/// (`gitpm::ForkStep`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ForkStep {
    /// Committing/tagging the fork's first release.
    Release,
    /// Installing the released fork.
    Install,
    /// Making the installed fork the provider of the original.
    Replace,
}
impl From<pm::ForkStep> for ForkStep {
    fn from(step: pm::ForkStep) -> Self {
        match step {
            pm::ForkStep::Release => Self::Release,
            pm::ForkStep::Install => Self::Install,
            pm::ForkStep::Replace => Self::Replace,
        }
    }
}
/// What `forkAndReplace`/`forkPackagesAndReplace` did for one package
/// (`gitpm::ForkReplacement`). The fork repository is kept; a failure after it exists is
/// reported here (`failedStep`, `errorKind`, `errorMessage`) with what was done so far,
/// instead of as an error.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkReplacement {
    pub original: String,
    pub id: String,
    pub path: String,
    pub tag: Option<String>,
    pub installed: Option<InstallResult>,
    pub replacement: Option<ReplacementPlan>,
    pub failed_step: Option<ForkStep>,
    /// Stable kind of the failure (as `PmError` kinds), with its message.
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}
/// A dependency rewritten by a multi-package fork (`from` became `to`); `newRequirement`
/// is set when the old requirement does not accept the fork's version.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DependencyRewrite {
    pub package: String,
    pub from: String,
    pub to: String,
    pub requirement: String,
    pub new_requirement: Option<String>,
}
impl From<pm::DependencyRewrite> for DependencyRewrite {
    fn from(r: pm::DependencyRewrite) -> Self {
        Self {
            package: r.package,
            from: r.from,
            to: r.to,
            requirement: r.requirement,
            new_requirement: r.new_requirement,
        }
    }
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkedPackage {
    pub original: String,
    pub id: String,
    /// Package folder inside the fork repository; empty for a root package.
    pub path: String,
    pub version: String,
}
/// Result of `PluginManager.forkPackages` (`gitpm::PackageFork`): `created` is false
/// when the packages were added to an existing multi-package repository.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PackageFork {
    pub path: String,
    pub created: bool,
    pub commit: String,
    pub packages: Vec<ForkedPackage>,
    pub rewrites: Vec<DependencyRewrite>,
}
impl From<pm::PackageFork> for PackageFork {
    fn from(f: pm::PackageFork) -> Self {
        Self {
            path: f.path.to_string_lossy().into_owned(),
            created: f.created,
            commit: f.commit,
            packages: f
                .packages
                .into_iter()
                .map(|p| ForkedPackage {
                    original: p.original,
                    id: p.id,
                    path: p.path,
                    version: p.version,
                })
                .collect(),
            rewrites: f.rewrites.into_iter().map(Into::into).collect(),
        }
    }
}
/// Result of `PluginManager.forkPackagesAndReplace` (`gitpm::PackageForkReplacement`):
/// one entry per package (install order), each with its own
/// `failedStep`/`errorKind`/`errorMessage`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PackageForkReplacement {
    pub fork: PackageFork,
    pub packages: Vec<ForkReplacement>,
}
impl From<pm::PackageForkReplacement> for PackageForkReplacement {
    fn from(r: pm::PackageForkReplacement) -> Self {
        Self {
            fork: r.fork.into(),
            packages: r.packages.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<pm::ForkReplacement> for ForkReplacement {
    fn from(r: pm::ForkReplacement) -> Self {
        Self {
            original: r.original,
            id: r.id,
            path: r.path.to_string_lossy().into_owned(),
            tag: r.tag,
            installed: r.installed.map(Into::into),
            replacement: r.replacement.map(Into::into),
            failed_step: r.failed.map(Into::into),
            error_kind: r.error.as_ref().map(|e| e.kind().to_owned()),
            error_message: r.error.as_ref().map(ToString::to_string),
        }
    }
}

/// Stable: latest release tag (or an explicit ref). Dev: follow the branch HEAD.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum Channel {
    Stable,
    Dev,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct ManagerOptions {
    /// Catalog location: local folder, JSON file or HTTP(S) URL.
    #[uniffi(default = None)]
    pub catalog: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Manager object

/// An open, locked workspace. Thread-safe; operations are serialized internally.
#[derive(uniffi::Object)]
pub struct PluginManager {
    inner: pm::Manager,
}

#[uniffi::export]
impl PluginManager {
    /// Open (or create) a dedicated workspace directory and take its lock.
    #[uniffi::constructor]
    pub fn open(root: String, options: ManagerOptions) -> Result<Arc<Self>> {
        let inner = pm::Manager::open_with(
            root,
            pm::Options {
                catalog: options.catalog,
            },
        )?;
        Ok(Arc::new(Self { inner }))
    }
    pub fn root(&self) -> String {
        self.inner.root().to_string_lossy().into_owned()
    }
    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        Ok(self.inner.list()?.into_iter().map(Into::into).collect())
    }
    /// Compare installed versions with the configured catalog (read-only).
    pub fn check(&self) -> Result<Vec<CheckRow>> {
        Ok(self.inner.check()?.into_iter().map(Into::into).collect())
    }
    pub fn browse_catalog(&self) -> Result<CatalogOverview> {
        Ok(self.inner.browse_catalog()?.into())
    }
    /// Release from the configured catalog; latest if `version` is `None`.
    pub fn catalog_release(&self, id: String, version: Option<String>) -> Result<CatalogRelease> {
        Ok(self.inner.catalog_release(&id, version.as_deref())?.into())
    }
    /// Install from a Git source (URL or local path). `package` picks a package of a
    /// repository with `gitpm.json` (needed when it lists several, unless `reference` is
    /// an `ID@VERSION` tag); for a single-package repository it must match.
    #[uniffi::method(default(package = None))]
    pub fn install(
        &self,
        source: String,
        reference: Option<String>,
        channel: Channel,
        package: Option<String>,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .install_package(
                &source,
                package.as_deref(),
                reference.as_deref(),
                matches!(channel, Channel::Dev),
            )?
            .into())
    }
    /// Install an exact catalog release (latest if `version` is `None`), verifying hashes.
    pub fn install_from_catalog(
        &self,
        id: String,
        version: Option<String>,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .install_from_catalog(&id, version.as_deref())?
            .into())
    }
    pub fn update(
        &self,
        id: String,
        reference: Option<String>,
        channel: Channel,
    ) -> Result<RevisionChange> {
        Ok(self
            .inner
            .update_channel(&id, reference.as_deref(), matches!(channel, Channel::Dev))?
            .into())
    }
    pub fn update_all_plan(&self) -> Result<UpdateAllPlan> {
        Ok(self.inner.update_all_plan()?.into())
    }
    pub fn update_all(&self) -> Result<UpdateAllPlan> {
        Ok(self.inner.update_all()?.into())
    }
    pub fn rollback(&self, id: String) -> Result<RevisionChange> {
        Ok(self.inner.rollback(&id)?.into())
    }
    pub fn enable(&self, id: String) -> Result<()> {
        Ok(self.inner.enable(&id)?)
    }
    /// Refuses with `HasDependents` if others depend on it, unless `force`.
    #[uniffi::method(default(force = false))]
    pub fn disable(&self, id: String, force: bool) -> Result<()> {
        Ok(self.inner.set_enabled_with(&id, false, force)?)
    }
    /// Unregister; repository and deployed files stay on disk. Dependents refuse
    /// unless `force`.
    #[uniffi::method(default(force = false))]
    pub fn forget(&self, id: String, force: bool) -> Result<()> {
        Ok(self.inner.forget_with(&id, force)?)
    }
    /// Re-register a forgotten repository.
    pub fn restore(&self, id: String) -> Result<InstallResult> {
        Ok(self.inner.restore(&id)?.into())
    }
    pub fn uninstall_plan(&self, id: String) -> Result<UninstallPlan> {
        Ok(self.inner.uninstall_plan(&id)?.into())
    }
    /// `confirm = false` refuses with `LocalChanges` if anything would be lost;
    /// dependents refuse with `HasDependents` unless `force`.
    #[uniffi::method(default(force = false))]
    pub fn uninstall(&self, id: String, confirm: bool, force: bool) -> Result<()> {
        Ok(self.inner.uninstall_with(&id, confirm, force)?)
    }
    /// Read-only: what replacing `original` by `replacement` would change.
    pub fn replace_plan(&self, original: String, replacement: String) -> Result<ReplacementPlan> {
        Ok(self.inner.replace_plan(&original, &replacement)?.into())
    }
    /// Make `replacement` the provider of `original` for every consumer. Unsatisfied
    /// consumers refuse with `IncompatibleReplacement` unless `confirm`.
    pub fn replace(
        &self,
        original: String,
        replacement: String,
        confirm: bool,
    ) -> Result<ReplacementPlan> {
        Ok(self.inner.replace(&original, &replacement, confirm)?.into())
    }
    pub fn switch_back_plan(&self, original: String) -> Result<ReplacementPlan> {
        Ok(self.inner.switch_back_plan(&original)?.into())
    }
    /// Every consumer uses the installed original again (same confirmation rule).
    pub fn switch_back(&self, original: String, confirm: bool) -> Result<ReplacementPlan> {
        Ok(self.inner.switch_back(&original, confirm)?.into())
    }
    /// Fork an installed package as `namespace/<same name>` into the fork repository
    /// `destination` (outside the workspace): a new folder, or an existing repository with
    /// `gitpm.json` to add it to. The one-package case of `forkPackages`.
    pub fn fork(&self, id: String, destination: String, namespace: String) -> Result<ForkResult> {
        let path =
            self.inner
                .fork_in_namespace(&id, std::path::Path::new(&destination), &namespace)?;
        let name = id.rsplit('/').next().unwrap_or(&id);
        Ok(ForkResult {
            id: pm::identity::qualified(&namespace, name)?,
            path: path.to_string_lossy().into_owned(),
        })
    }
    /// Fork, release the fork (`version` defaults to the forked one), install it and
    /// replace `id` with it for every consumer (`confirm` as in `replace`). Errors before
    /// the fork folder exists are thrown; later failures are reported in the result.
    #[uniffi::method(default(version = None, confirm = false))]
    pub fn fork_and_replace(
        &self,
        id: String,
        destination: String,
        namespace: String,
        version: Option<String>,
        confirm: bool,
    ) -> Result<ForkReplacement> {
        Ok(self
            .inner
            .fork_replace(
                &id,
                std::path::Path::new(&destination),
                &namespace,
                version.as_deref(),
                confirm,
            )?
            .into())
    }
    /// Fork installed packages of one repository into `namespace` as the fork repository
    /// `destination`: a new folder (a copy of the source with the unforked siblings
    /// removed), or an existing repository with `gitpm.json` to add them to; anything
    /// else is refused. Dependencies between the forks are rewritten.
    pub fn fork_packages(
        &self,
        ids: Vec<String>,
        destination: String,
        namespace: String,
    ) -> Result<PackageFork> {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        Ok(self
            .inner
            .fork_packages(&ids, std::path::Path::new(&destination), &namespace)?
            .into())
    }
    /// `forkPackages`, then release, install and replace every package. Errors before the
    /// fork exists are thrown; later failures are reported per package.
    #[uniffi::method(default(version = None, confirm = false))]
    pub fn fork_packages_and_replace(
        &self,
        ids: Vec<String>,
        destination: String,
        namespace: String,
        version: Option<String>,
        confirm: bool,
    ) -> Result<PackageForkReplacement> {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        Ok(self
            .inner
            .fork_packages_replace(
                &ids,
                std::path::Path::new(&destination),
                &namespace,
                version.as_deref(),
                confirm,
            )?
            .into())
    }
}

// ---------------------------------------------------------------------------------------
// Free functions (no workspace needed)

/// Check read access to a Git repository with the user's credentials; nothing is cloned.
/// `custom_ssh`: the process environment already sets GIT_SSH_COMMAND/GIT_SSH.
#[uniffi::export]
pub fn auth_check(source: String, custom_ssh: bool) -> Result<AuthCheck> {
    let result = pm::auth::check(&source, &pm::auth::AuthOptions { custom_ssh })?;
    Ok(AuthCheck {
        source: result.source,
        message: result.message,
    })
}

/// Bounded overview of any catalog location.
#[uniffi::export]
pub fn browse_catalog(location: String) -> Result<CatalogOverview> {
    Ok(pm::catalog::browse(&location)?.into())
}

/// Release of `id` in a catalog location; latest if `version` is `None`.
#[uniffi::export]
pub fn catalog_release(
    location: String,
    id: String,
    version: Option<String>,
) -> Result<CatalogRelease> {
    Ok(pm::catalog::lookup(&location, &id, version.as_deref())?.into())
}

/// Read installed state without opening (locking or creating) the workspace.
#[uniffi::export]
pub fn read_installed(root: String) -> Result<Vec<InstalledPlugin>> {
    let root = std::path::Path::new(&root);
    Ok(pm::check::read_state(root)?
        .installed_in(root)
        .into_iter()
        .map(Into::into)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_keep_kind_and_message() {
        let error: PmError = pm::Error::NotInstalled { id: "a/b".into() }.into();
        assert!(matches!(&error, PmError::NotInstalled { id, .. } if id == "a/b"));
        assert_eq!(error.to_string(), "plugin a/b is not installed");
        let error: PmError = pm::Error::LocalChanges {
            id: "a/b".into(),
            message: "dirty".into(),
            items: vec![pm::LostWork::Stash {
                entry: "stash@{0}".into(),
            }],
        }
        .into();
        assert!(matches!(&error, PmError::LocalChanges { items, .. } if items.len() == 1));
    }
    #[test]
    fn manager_round_trip_on_empty_workspace() {
        let dir = std::env::temp_dir().join(format!("pm-ffi-{}", std::process::id()));
        let root = dir.join("workspace");
        let manager =
            PluginManager::open(root.to_string_lossy().into(), ManagerOptions::default()).unwrap();
        assert!(manager.list().unwrap().is_empty());
        assert!(matches!(
            manager.uninstall_plan("a/b".into()),
            Err(PmError::RepositoryNotFound { .. })
        ));
        assert!(matches!(manager.check(), Ok(rows) if rows.is_empty()));
        assert!(matches!(
            PluginManager::open(root.to_string_lossy().into(), ManagerOptions::default()),
            Err(PmError::Locked { .. })
        ));
        assert!(manager.update_all_plan().unwrap().updates.is_empty());
        assert!(manager.update_all().unwrap().installs.is_empty());
        drop(manager);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Every library error with the FFI case it must become (`kind` of the case).
    fn library_errors() -> Vec<pm::Error> {
        let m = || "msg".to_owned();
        vec![
            pm::Error::NotInstalled { id: "a/b".into() },
            pm::Error::AlreadyInstalled {
                id: "a/b".into(),
                message: m(),
            },
            pm::Error::LocalChanges {
                id: "a/b".into(),
                message: m(),
                items: vec![],
            },
            pm::Error::Locked(m()),
            pm::Error::InvalidId(m()),
            pm::Error::InvalidManifest(m()),
            pm::Error::InvalidVersion(m()),
            pm::Error::InvalidArgument(m()),
            pm::Error::VersionConflict(m()),
            pm::Error::NoPreviousVersion { id: "a/b".into() },
            pm::Error::NoReleases(m()),
            pm::Error::RepositoryNotFound { id: "a/b".into() },
            pm::Error::Workspace(m()),
            pm::Error::Catalog(m()),
            pm::Error::Git(m()),
            pm::Error::Http(m()),
            pm::Error::Auth(m()),
            pm::Error::Unsupported(m()),
            pm::Error::DependencyConflict(m()),
            pm::Error::HasDependents {
                id: "x/one".into(),
                message: m(),
                dependents: vec!["app/a".into()],
            },
            pm::Error::IncompatibleReplacement {
                id: "x/one".into(),
                message: m(),
                consumers: vec!["app/b".into()],
            },
            pm::Error::InvalidData(m()),
            pm::Error::Io(m()),
        ]
    }
    /// Stable kind of an FFI error case (exhaustive: a new case must be added here).
    fn ffi_kind(error: &PmError) -> String {
        match error {
            PmError::NotInstalled { .. } => "not_installed",
            PmError::AlreadyInstalled { .. } => "already_installed",
            PmError::LocalChanges { .. } => "local_changes",
            PmError::Locked { .. } => "locked",
            PmError::InvalidId { .. } => "invalid_id",
            PmError::InvalidManifest { .. } => "invalid_manifest",
            PmError::InvalidVersion { .. } => "invalid_version",
            PmError::InvalidArgument { .. } => "invalid_argument",
            PmError::VersionConflict { .. } => "version_conflict",
            PmError::NoPreviousVersion { .. } => "no_previous_version",
            PmError::NoReleases { .. } => "no_releases",
            PmError::RepositoryNotFound { .. } => "repository_not_found",
            PmError::Workspace { .. } => "workspace",
            PmError::Catalog { .. } => "catalog",
            PmError::Git { .. } => "git",
            PmError::Http { .. } => "http",
            PmError::Auth { .. } => "auth",
            PmError::Unsupported { .. } => "unsupported",
            PmError::DependencyConflict { .. } => "dependency_conflict",
            PmError::HasDependents { .. } => "has_dependents",
            PmError::IncompatibleReplacement { .. } => "incompatible_replacement",
            PmError::InvalidData { .. } => "invalid_data",
            PmError::Io { .. } => "io",
            PmError::Other { kind, .. } => return format!("other:{kind}"),
        }
        .into()
    }
    fn ffi_message(error: &PmError) -> &str {
        match error {
            PmError::NotInstalled { message, .. }
            | PmError::AlreadyInstalled { message, .. }
            | PmError::LocalChanges { message, .. }
            | PmError::Locked { message }
            | PmError::InvalidId { message }
            | PmError::InvalidManifest { message }
            | PmError::InvalidVersion { message }
            | PmError::InvalidArgument { message }
            | PmError::VersionConflict { message }
            | PmError::NoPreviousVersion { message, .. }
            | PmError::NoReleases { message }
            | PmError::RepositoryNotFound { message, .. }
            | PmError::Workspace { message }
            | PmError::Catalog { message }
            | PmError::Git { message }
            | PmError::Http { message }
            | PmError::Auth { message }
            | PmError::Unsupported { message }
            | PmError::DependencyConflict { message }
            | PmError::HasDependents { message, .. }
            | PmError::IncompatibleReplacement { message, .. }
            | PmError::InvalidData { message }
            | PmError::Io { message }
            | PmError::Other { message, .. } => message,
        }
    }

    #[test]
    fn every_library_error_maps_to_its_own_case_with_the_full_message() {
        let errors = library_errors();
        assert_eq!(errors.len(), 23);
        for error in errors {
            let kind = error.kind();
            let display = error.to_string();
            let mapped = PmError::from(error);
            assert_eq!(ffi_kind(&mapped), kind, "{mapped:?}");
            assert_eq!(ffi_message(&mapped), display);
            assert_eq!(mapped.to_string(), display);
        }
    }

    #[test]
    fn ids_and_lost_work_items_are_preserved() {
        let id_of = |error: PmError| match error {
            PmError::NotInstalled { id, .. }
            | PmError::AlreadyInstalled { id, .. }
            | PmError::LocalChanges { id, .. }
            | PmError::NoPreviousVersion { id, .. }
            | PmError::RepositoryNotFound { id, .. } => id,
            other => panic!("no id: {other:?}"),
        };
        for error in [
            pm::Error::NotInstalled { id: "x/one".into() },
            pm::Error::AlreadyInstalled {
                id: "x/one".into(),
                message: "m".into(),
            },
            pm::Error::NoPreviousVersion { id: "x/one".into() },
            pm::Error::RepositoryNotFound { id: "x/one".into() },
        ] {
            assert_eq!(id_of(error.into()), "x/one");
        }
        let items = all_lost_work();
        let error = PmError::from(pm::Error::LocalChanges {
            id: "x/one".into(),
            message: "local commits detected".into(),
            items: items.clone(),
        });
        let PmError::LocalChanges {
            id,
            message,
            items: mapped,
        } = error
        else {
            panic!()
        };
        assert_eq!(
            (id.as_str(), message.as_str()),
            ("x/one", "local commits detected")
        );
        let back: Vec<pm::LostWork> = mapped.into_iter().map(Into::into).collect();
        assert_eq!(back, items, "order and content are kept");
    }

    fn all_lost_work() -> Vec<pm::LostWork> {
        vec![
            pm::LostWork::Modified { path: "m".into() },
            pm::LostWork::Untracked { path: "u".into() },
            pm::LostWork::Ignored { path: "i".into() },
            pm::LostWork::HeadMoved {
                head: "h".into(),
                recorded: "r".into(),
            },
            pm::LostWork::UnpublishedCommit {
                commit: "c".into(),
                summary: "s".into(),
            },
            pm::LostWork::UnpublishedBranch {
                name: "b".into(),
                commits: 7,
            },
            pm::LostWork::UnpublishedTag { name: "t".into() },
            pm::LostWork::Stash {
                entry: "stash@{0}".into(),
            },
        ]
    }

    #[test]
    fn lost_work_round_trips_and_describes_like_the_library() {
        for item in all_lost_work() {
            let ffi = LostWork::from(item.clone());
            assert_eq!(describe_lost_work(ffi.clone()), item.describe());
            assert_eq!(pm::LostWork::from(ffi), item);
        }
    }

    fn revision(v: &str) -> pm::Revision {
        pm::Revision {
            version: v.into(),
            commit: format!("{v}-commit"),
        }
    }
    fn fork_of() -> pm::ForkOf {
        pm::ForkOf {
            id: "vendor/x".into(),
            version: "0.9.0".into(),
            commit: "f".repeat(40),
        }
    }

    #[test]
    fn fork_replace_result_reports_the_failed_step_with_kind_and_message() {
        let result = ForkReplacement::from(pm::ForkReplacement {
            original: "vendor/x".into(),
            id: "me/x".into(),
            path: "/forks/me/x".into(),
            tag: Some("me/x@1.0.0".into()),
            installed: None,
            replacement: None,
            failed: Some(pm::ForkStep::Install),
            error: Some(pm::Error::Unsupported("symlinks are not supported".into())),
        });
        assert_eq!(result.failed_step, Some(ForkStep::Install));
        assert_eq!(result.error_kind.as_deref(), Some("unsupported"));
        assert_eq!(
            result.error_message.as_deref(),
            Some("symlinks are not supported")
        );
        assert_eq!(
            (result.path.as_str(), result.tag.as_deref()),
            ("/forks/me/x", Some("me/x@1.0.0"))
        );
    }

    #[test]
    fn records_convert_field_by_field() {
        let plugin = InstalledPlugin::from(pm::InstalledPlugin {
            id: "me/x".into(),
            source: "src".into(),
            current: revision("1.0.0"),
            previous: Some(revision("0.9.0")),
            enabled: false,
            fork_of: Some(fork_of()),
            replaces: None,
            catalog: None,
            path: Some("packages/x".into()),
            clone: Some("mono-0123456789abcdef".into()),
            title: Some("X".into()),
            description: None,
            provides: vec!["theme".into()],
            tags: vec!["ui".into(), "dark".into()],
            dependencies: vec![],
            replacing: vec![],
            dependents: vec![],
        });
        assert_eq!(
            (plugin.id.as_str(), plugin.source.as_str(), plugin.enabled),
            ("me/x", "src", false)
        );
        assert_eq!(plugin.path.as_deref(), Some("packages/x"));
        assert_eq!(plugin.clone.as_deref(), Some("mono-0123456789abcdef"));
        assert_eq!(
            (plugin.title.as_deref(), plugin.description.as_deref()),
            (Some("X"), None)
        );
        assert_eq!(plugin.provides, ["theme"]);
        assert_eq!(plugin.tags, ["ui", "dark"]);
        assert_eq!(
            (
                plugin.current.version.as_str(),
                plugin.current.commit.as_str()
            ),
            ("1.0.0", "1.0.0-commit")
        );
        assert_eq!(plugin.previous.unwrap().version, "0.9.0");
        let f = plugin.fork_of.unwrap();
        assert_eq!(
            (f.id.as_str(), f.version.as_str(), f.commit),
            ("vendor/x", "0.9.0", "f".repeat(40))
        );

        let result = InstallResult::from(pm::InstallResult {
            id: "me/x".into(),
            source: "src".into(),
            current: revision("1.0.0"),
            fork_of: None,
            skipped: Vec::new(),
            warnings: Vec::new(),
        });
        assert!(result.fork_of.is_none());
        assert_eq!(result.current.commit, "1.0.0-commit");

        let change = RevisionChange::from(pm::RevisionChange {
            id: "me/x".into(),
            from: revision("1.0.0"),
            to: revision("1.1.0"),
            changed: true,
            warnings: Vec::new(),
        });
        assert!(change.changed);
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("1.0.0", "1.1.0")
        );

        let plan = UninstallPlan::from(pm::UninstallPlan {
            id: "me/x".into(),
            registered: false,
            deletes: vec!["/w/plugins/me/x".into(), "/w/dist/me/x".into()],
            lost_work: all_lost_work(),
            dependents: vec![],
        });
        assert!(!plan.registered);
        assert_eq!(plan.deletes, ["/w/plugins/me/x", "/w/dist/me/x"]);
        assert_eq!(plan.lost_work.len(), 8);
        assert!(matches!(
            plan.lost_work[5],
            LostWork::UnpublishedBranch { commits: 7, .. }
        ));
    }

    #[test]
    fn check_rows_carry_status_and_message() {
        use pm::check::CheckStatus as S;
        let statuses = [
            (S::NotInCatalog, "NotInCatalog"),
            (S::DifferentSource, "DifferentSource"),
            (S::InvalidCatalogData, "InvalidCatalogData"),
            (S::NoVersions, "NoVersions"),
            (S::UncomparableVersion, "UncomparableVersion"),
            (S::SameVersionDifferentCommit, "SameVersionDifferentCommit"),
            (S::UpdateAvailable, "UpdateAvailable"),
            (S::UpToDate, "UpToDate"),
            (S::NewerThanCatalog, "NewerThanCatalog"),
            (S::OtherCatalog, "OtherCatalog"),
        ];
        for (status, name) in statuses {
            let row = CheckRow::from(pm::check::CheckRow {
                id: "me/x".into(),
                installed: "1.0.0".into(),
                latest: Some("1.1.0".into()),
                enabled: false,
                update_available: status == S::UpdateAvailable,
                status,
                upstream: None,
            });
            assert_eq!(format!("{:?}", row.status), name);
            assert_eq!(row.status_message, status.message());
            assert_eq!(row.update_available, status == S::UpdateAvailable);
            assert_eq!(
                (row.installed.as_str(), row.latest.as_deref(), row.enabled),
                ("1.0.0", Some("1.1.0"), false)
            );
        }
    }

    #[test]
    fn catalog_records_convert() {
        use pm::catalog::{
            Build, CatalogFormat as F, CatalogOverview as O, FileDigest, OverviewItem as I,
            PluginSummary as P, Release,
        };
        let overview = CatalogOverview::from(O {
            location: "https://pkg.example".into(),
            remote: true,
            format: F::V2,
            items: vec![
                I::Directory {
                    depth: 1,
                    name: "namespaces".into(),
                },
                I::Empty { depth: 2 },
                I::Plugin {
                    depth: 3,
                    plugin: P {
                        id: "dan/x".into(),
                        source: "src".into(),
                        versions: vec!["1.9.0".into(), "1.10.0".into()],
                        latest: Some("1.10.0".into()),
                        title: Some("X".into()),
                        description: Some("An x".into()),
                        provides: vec!["theme".into()],
                        tags: vec![],
                    },
                },
            ],
            truncated: true,
        });
        assert_eq!(
            (
                overview.location.as_str(),
                overview.remote,
                overview.truncated
            ),
            ("https://pkg.example", true, true)
        );
        assert!(matches!(overview.format, CatalogFormat::V2));
        assert!(
            matches!(&overview.items[0], OverviewItem::Directory { depth: 1, name } if name == "namespaces")
        );
        assert!(matches!(
            overview.items[1],
            OverviewItem::Empty { depth: 2 }
        ));
        let OverviewItem::Plugin { depth: 3, plugin } = &overview.items[2] else {
            panic!()
        };
        assert_eq!(plugin.versions, ["1.9.0", "1.10.0"]);
        assert_eq!(plugin.latest.as_deref(), Some("1.10.0"));
        assert_eq!(
            (plugin.title.as_deref(), plugin.description.as_deref()),
            (Some("X"), Some("An x"))
        );
        assert_eq!(plugin.provides, ["theme"]);
        assert!(plugin.tags.is_empty());
        let v1 = CatalogOverview::from(O {
            location: "l".into(),
            remote: false,
            format: F::V1,
            items: vec![],
            truncated: false,
        });
        assert!(matches!(v1.format, CatalogFormat::V1) && v1.items.is_empty());

        let mut files = std::collections::BTreeMap::new();
        for name in ["a.js", "b/c.js"] {
            files.insert(
                name.to_owned(),
                FileDigest {
                    sha256: "0".repeat(64),
                    size: 1,
                },
            );
        }
        let mut extra = pm::Manifest::default().extra;
        extra.insert("title".into(), "X".into());
        extra.insert("provides".into(), "theme".into());
        extra.insert("tags".into(), vec!["a"].into());
        let release = CatalogRelease::from(Release {
            manifest: pm::Manifest {
                id: "me/x".into(),
                version: "1.0.0".into(),
                fork_of: Some(fork_of()),
                extra,
                ..Default::default()
            },
            source: "src".into(),
            commit: "c".repeat(40),
            reference: "me/x@1.0.0".into(),
            files,
            build: Some(Build {
                url: "https://pkg.example/x.tar.zst".into(),
                sha256: "d".repeat(64),
            }),
            path: Some("packages/x".into()),
        });
        assert_eq!(
            (
                release.id.as_str(),
                release.version.as_str(),
                release.file_count
            ),
            ("me/x", "1.0.0", 2)
        );
        assert_eq!(release.reference, "me/x@1.0.0");
        assert_eq!(release.path.as_deref(), Some("packages/x"));
        assert_eq!(release.fork_of.unwrap().id, "vendor/x");
        assert_eq!(
            release.build_url.as_deref(),
            Some("https://pkg.example/x.tar.zst")
        );
        assert_eq!(release.build_sha256, Some("d".repeat(64)));
        assert_eq!(release.title.as_deref(), Some("X"));
        assert!(release.description.is_none());
        assert_eq!(
            (release.provides, release.tags),
            (vec!["theme".into()], vec!["a".into()])
        );
        let bare = CatalogRelease::from(Release {
            manifest: pm::Manifest {
                id: "x".into(),
                version: "1.0.0".into(),
                ..Default::default()
            },
            source: "s".into(),
            commit: "c".repeat(40),
            reference: "v1.0.0".into(),
            files: Default::default(),
            build: None,
            path: None,
        });
        assert!(bare.path.is_none());
        assert!(bare.build_url.is_none() && bare.build_sha256.is_none() && bare.fork_of.is_none());
        assert_eq!(bare.file_count, 0);
        assert!(bare.title.is_none() && bare.provides.is_empty() && bare.tags.is_empty());
    }

    #[test]
    fn free_functions_return_ffi_errors() {
        assert!(matches!(
            auth_check(String::new(), true),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            browse_catalog("ftp://x".into()),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            catalog_release("/nonexistent-pm-catalog".into(), "../x".into(), None),
            Err(PmError::InvalidId { .. })
        ));
        assert!(matches!(
            catalog_release(
                "/nonexistent-pm-catalog".into(),
                "x".into(),
                Some("1.0".into())
            ),
            Err(PmError::InvalidVersion { .. })
        ));
        assert!(matches!(
            catalog_release("/nonexistent-pm-catalog".into(), "x".into(), None),
            Err(PmError::Catalog { .. })
        ));
        // A workspace that does not exist yet reads as empty and is not created.
        let missing = std::env::temp_dir().join(format!("pm-ffi-missing-{}", std::process::id()));
        assert!(
            read_installed(missing.to_string_lossy().into())
                .unwrap()
                .is_empty()
        );
        assert!(!missing.exists());
    }

    #[test]
    fn read_installed_and_manager_report_state_errors() {
        let dir = std::env::temp_dir().join(format!("pm-ffi-state-{}", std::process::id()));
        let root = dir.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("state.json"),
            r#"{"plugins":{"me/x":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":false,"forkOf":{"id":"vendor/x","version":"0.9.0","commit":"f"}}}}"#,
        )
        .unwrap();
        let list = read_installed(root.to_string_lossy().into()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), list[0].enabled), ("me/x", false));
        assert_eq!(list[0].fork_of.as_ref().unwrap().id, "vendor/x");

        let manager =
            PluginManager::open(root.to_string_lossy().into(), ManagerOptions::default()).unwrap();
        assert_eq!(manager.root(), root.to_string_lossy());
        assert_eq!(manager.list().unwrap()[0].current.version, "1.0.0");
        assert!(matches!(
            manager.check(),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.browse_catalog(),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.enable("../x".into()),
            Err(PmError::InvalidId { .. })
        ));
        assert!(
            matches!(manager.rollback("me/missing".into()), Err(PmError::NotInstalled { id, .. }) if id == "me/missing")
        );
        assert!(matches!(
            manager.install("x".into(), Some("v1.0.0".into()), Channel::Dev, None),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.update("me/x".into(), Some("main".into()), Channel::Dev),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.fork(
                "me/x".into(),
                dir.join("fork").to_string_lossy().into(),
                "bad ns".into()
            ),
            Err(PmError::InvalidId { .. })
        ));
        assert!(matches!(
            manager.fork_packages(
                vec!["me/x".into(), "me/y".into()],
                dir.join("fork").to_string_lossy().into(),
                "you".into()
            ),
            Err(PmError::NotInstalled { .. })
        ));
        assert!(matches!(
            manager.fork_packages(
                vec![],
                dir.join("fork").to_string_lossy().into(),
                "you".into()
            ),
            Err(PmError::InvalidArgument { .. })
        ));
        drop(manager);

        std::fs::write(root.join("state.json"), "{broken").unwrap();
        assert!(matches!(
            read_installed(root.to_string_lossy().into()),
            Err(PmError::InvalidData { .. })
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
