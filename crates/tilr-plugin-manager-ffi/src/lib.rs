//! UniFFI bindings for native apps (Swift/Kotlin). Consumer operations only: open a
//! workspace, browse a catalog, install/update/rollback/enable/disable/forget/restore,
//! plan and confirm uninstall, fork, check repository access.
//!
//! All calls block (Git/network); call them off the main thread.
use std::sync::Arc;
use tilr_plugin_manager as pm;

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

#[derive(Debug, Clone, uniffi::Record)]
pub struct InstalledPlugin {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    pub fork_of: Option<ForkOf>,
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
}
impl From<pm::UninstallPlan> for UninstallPlan {
    fn from(p: pm::UninstallPlan) -> Self {
        Self {
            id: p.id,
            registered: p.registered,
            deletes: p.deletes,
            lost_work: p.lost_work.into_iter().map(Into::into).collect(),
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
    pub build_url: Option<String>,
    pub build_sha256: Option<String>,
}
impl From<pm::catalog::Release> for CatalogRelease {
    fn from(r: pm::catalog::Release) -> Self {
        Self {
            id: r.manifest.id,
            version: r.manifest.version,
            source: r.source,
            commit: r.commit,
            reference: r.reference,
            file_count: r.files.len() as u64,
            fork_of: r.manifest.fork_of.map(Into::into),
            build_url: r.build.as_ref().map(|b| b.url.clone()),
            build_sha256: r.build.map(|b| b.sha256),
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
    /// Install from a Git source (URL or local path).
    pub fn install(
        &self,
        source: String,
        reference: Option<String>,
        channel: Channel,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .install_channel(
                &source,
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
    pub fn rollback(&self, id: String) -> Result<RevisionChange> {
        Ok(self.inner.rollback(&id)?.into())
    }
    pub fn enable(&self, id: String) -> Result<()> {
        Ok(self.inner.enable(&id)?)
    }
    pub fn disable(&self, id: String) -> Result<()> {
        Ok(self.inner.disable(&id)?)
    }
    /// Unregister; repository and deployed files stay on disk.
    pub fn forget(&self, id: String) -> Result<()> {
        Ok(self.inner.forget(&id)?)
    }
    /// Re-register a forgotten repository.
    pub fn restore(&self, id: String) -> Result<InstallResult> {
        Ok(self.inner.restore(&id)?.into())
    }
    pub fn uninstall_plan(&self, id: String) -> Result<UninstallPlan> {
        Ok(self.inner.uninstall_plan(&id)?.into())
    }
    /// `confirm = false` refuses with `LocalChanges` if anything would be lost.
    pub fn uninstall(&self, id: String, confirm: bool) -> Result<()> {
        Ok(self.inner.uninstall(&id, confirm)?)
    }
    /// Fork an installed plugin into `namespace` at `destination` (outside the workspace).
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
    Ok(pm::check::read_state(std::path::Path::new(&root))?
        .installed()
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
        assert_eq!(error.to_string(), "plugin is not installed");
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
        drop(manager);
        let _ = std::fs::remove_dir_all(dir);
    }
}
