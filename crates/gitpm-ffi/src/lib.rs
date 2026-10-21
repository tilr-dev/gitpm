//! UniFFI bindings for native apps (Swift/Kotlin). Consumer operations only: open a
//! workspace, browse a catalog, install/update/rollback/enable/disable/forget/restore,
//! plan and confirm uninstall, fork, replace a package by a fork and switch back,
//! bring `dist/` in line with state (`syncDist`), check repository access.
//!
//! `dist/<id>` exists exactly for the active packages (installed, enabled, not
//! replaced): a host loads everything under `dist/` without resolving replacements.
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
    InvalidManifest { id: Option<String>, message: String },
    #[error("{message}")]
    InvalidVersion { message: String },
    #[error("{message}")]
    InvalidArgument { message: String },
    #[error("{message}")]
    VersionConflict { id: Option<String>, message: String },
    /// `commit`: the previous revision's commit is no longer in the clone.
    #[error("{message}")]
    NoPreviousVersion {
        id: String,
        commit: Option<String>,
        message: String,
    },
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
    /// Installed packages that would break (`dependents`); retry with `force` to
    /// proceed anyway. `lost_work` is the local work an uninstall would also lose (empty
    /// for other operations), so one refusal tells everything.
    #[error("{message}")]
    HasDependents {
        id: String,
        message: String,
        dependents: Vec<String>,
        lost_work: Vec<LostWork>,
    },
    /// `consumers` do not accept the new provider's version; retry with `confirm`.
    #[error("{message}")]
    IncompatibleReplacement {
        id: String,
        message: String,
        consumers: Vec<String>,
    },
    /// `syncDist` committed what it could; `sync` is its report (as `syncDist` returns
    /// it), whose `skipped` are the active packages left without their deployed folder.
    #[error("{message}")]
    NotDeployed { message: String, sync: DistSync },
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
            pm::Error::InvalidManifest { id, .. } => PmError::InvalidManifest { id, message },
            pm::Error::InvalidVersion(_) => PmError::InvalidVersion { message },
            pm::Error::InvalidArgument(_) => PmError::InvalidArgument { message },
            pm::Error::VersionConflict { id, .. } => PmError::VersionConflict { id, message },
            pm::Error::NoPreviousVersion { id, commit } => PmError::NoPreviousVersion {
                id,
                commit,
                message,
            },
            pm::Error::NoReleases(_) => PmError::NoReleases { message },
            pm::Error::RepositoryNotFound { id } => PmError::RepositoryNotFound { id, message },
            pm::Error::Workspace(_) => PmError::Workspace { message },
            pm::Error::Catalog(_) => PmError::Catalog { message },
            pm::Error::Git(_) => PmError::Git { message },
            pm::Error::Http(_) => PmError::Http { message },
            pm::Error::Auth(_) => PmError::Auth { message },
            pm::Error::Unsupported(_) => PmError::Unsupported { message },
            pm::Error::DependencyConflict(_) => PmError::DependencyConflict { message },
            pm::Error::HasDependents {
                id,
                dependents,
                lost_work,
                ..
            } => PmError::HasDependents {
                id,
                message,
                dependents,
                lost_work: lost_work.into_iter().map(Into::into).collect(),
            },
            pm::Error::NotDeployed { sync, .. } => PmError::NotDeployed {
                message,
                sync: (*sync).into(),
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
    /// Only in `previous`: the mode the package had at that revision (rollback restores
    /// it); `release` everywhere else.
    pub mode: Mode,
}
impl From<pm::Revision> for Revision {
    fn from(r: pm::Revision) -> Self {
        Self {
            version: r.version,
            commit: r.commit,
            mode: r.mode.into(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkOf {
    pub id: String,
    pub version: String,
    pub commit: String,
    /// Recorded source of the original; `None` in manifests written without it.
    pub source: Option<String>,
}
impl From<pm::ForkOf> for ForkOf {
    fn from(f: pm::ForkOf) -> Self {
        Self {
            id: f.id,
            version: f.version,
            commit: f.commit,
            source: f.source,
        }
    }
}

/// The package can stand in for `id` at `version` (used only after an explicit replace).
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

/// One dependency of an installed package and the package that provides it.
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
pub struct InstalledPackage {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    /// What `update(id, .recorded)` follows: releases, a pinned ref or a branch.
    pub mode: Mode,
    /// Enabled and not replaced: exactly the packages with `dist/<id>` (what a host
    /// loads). A replaced package is `enabled` but not `active`.
    pub active: bool,
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
    /// Original IDs this package currently provides for every consumer.
    pub replacing: Vec<String>,
    /// Installed packages that depend on this one.
    pub dependents: Vec<String>,
}
impl From<pm::InstalledPackage> for InstalledPackage {
    fn from(p: pm::InstalledPackage) -> Self {
        Self {
            id: p.id,
            source: p.source,
            current: p.current.into(),
            previous: p.previous.map(Into::into),
            enabled: p.enabled,
            mode: p.mode.into(),
            active: p.active,
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

/// The recorded mode of an installed package (`mode` in `state.json`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Mode {
    /// Follows the newest release.
    Release,
    /// Fixed at a tag or commit, as the user gave it.
    Pinned { reference: String },
    /// Follows the HEAD of this branch of the origin.
    Branch { name: String },
}
impl From<pm::Mode> for Mode {
    fn from(mode: pm::Mode) -> Self {
        match mode {
            pm::Mode::Release => Mode::Release,
            pm::Mode::Pinned(reference) => Mode::Pinned { reference },
            pm::Mode::Branch(name) => Mode::Branch { name },
        }
    }
}

/// Why an update left a package where it was.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum UnchangedReason {
    /// Pinned to `reference`: only an explicit target moves it.
    Pinned { reference: String },
    /// Branch mode (`updateAll`): the branch history no longer contains the installed
    /// commit.
    Diverged { branch: String },
    /// Branch mode (`updateAll`): the origin no longer has the branch.
    BranchGone { branch: String },
    /// A broken consumer (`updateAll`): `dependency` is not satisfied (`problem`, for
    /// example `not installed` after a forced removal); it is never installed for it.
    BrokenDependency { dependency: String, problem: String },
    /// Protected by a verified catalog: the origin's newer release `version` is not in
    /// the catalog, so it is not installed.
    NotInCatalog { version: String },
    /// `updateAll`: the verified catalog of a protected package could not be used
    /// (`reason`); it stays.
    CatalogUnavailable { reason: String },
}
impl From<pm::UnchangedReason> for UnchangedReason {
    fn from(reason: pm::UnchangedReason) -> Self {
        match reason {
            pm::UnchangedReason::Pinned { reference } => UnchangedReason::Pinned { reference },
            pm::UnchangedReason::Diverged { branch } => UnchangedReason::Diverged { branch },
            pm::UnchangedReason::BranchGone { branch } => UnchangedReason::BranchGone { branch },
            pm::UnchangedReason::BrokenDependency {
                dependency,
                problem,
            } => UnchangedReason::BrokenDependency {
                dependency,
                problem,
            },
            pm::UnchangedReason::NotInCatalog { version } => {
                UnchangedReason::NotInCatalog { version }
            }
            pm::UnchangedReason::CatalogUnavailable { reason } => {
                UnchangedReason::CatalogUnavailable { reason }
            }
        }
    }
}

/// Why `updateAll` did not select the newest release of a package.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum HeldBackReason {
    /// Other packages' requirements do not allow the newer release.
    Constraints,
    /// The newest release needs `dependency` `requirement`, which is not installed, and
    /// no catalog is configured.
    MissingDependency {
        dependency: String,
        requirement: String,
    },
    /// The newest release of a replacing package no longer declares that it replaces
    /// `original`.
    NoLongerReplaces { original: String },
}
impl From<pm::HeldBackReason> for HeldBackReason {
    fn from(reason: pm::HeldBackReason) -> Self {
        match reason {
            pm::HeldBackReason::Constraints => HeldBackReason::Constraints,
            pm::HeldBackReason::MissingDependency {
                dependency,
                requirement,
            } => HeldBackReason::MissingDependency {
                dependency,
                requirement,
            },
            pm::HeldBackReason::NoLongerReplaces { original } => {
                HeldBackReason::NoLongerReplaces { original }
            }
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IncompatibleConsumer {
    pub id: String,
    pub requirement: String,
}
/// What replacing (or switching back) would change; see `PackageManager.replace`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReplacementPlan {
    pub original: String,
    pub provider: String,
    pub version: String,
    pub current: Option<String>,
    pub consumers: Vec<String>,
    /// Non-empty: ask the user, then call again with `confirm = true`.
    pub incompatible: Vec<IncompatibleConsumer>,
    /// Installed version of `provider` (the fork; the original when switching back).
    pub installed_version: Option<String>,
    /// Reconciliation report of `replace`/`switchBack` (empty for a plan).
    pub dist: DistSync,
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
            installed_version: p.installed_version,
            dist: p.dist.into(),
        }
    }
}
impl From<ReplacementPlan> for pm::ReplacementPlan {
    fn from(p: ReplacementPlan) -> Self {
        Self {
            original: p.original,
            provider: p.provider,
            version: p.version,
            current: p.current,
            consumers: p.consumers,
            incompatible: p
                .incompatible
                .into_iter()
                .map(|c| pm::IncompatibleConsumer {
                    id: c.id,
                    requirement: c.requirement,
                })
                .collect(),
            installed_version: p.installed_version,
            dist: p.dist.into(),
        }
    }
}
/// A replacement of `forkPackagesAndReplace` waiting for confirmation; pass it to
/// `PackageManager.forkReplaceResume` once the user confirmed.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PendingReplacement {
    pub original: String,
    pub fork: String,
    /// The plan to show (its `incompatible` consumers).
    pub plan: ReplacementPlan,
}
impl From<pm::PendingReplacement> for PendingReplacement {
    fn from(p: pm::PendingReplacement) -> Self {
        Self {
            original: p.original,
            fork: p.fork,
            plan: p.plan.into(),
        }
    }
}
impl From<PendingReplacement> for pm::PendingReplacement {
    fn from(p: PendingReplacement) -> Self {
        Self {
            original: p.original,
            fork: p.fork,
            plan: p.plan.into(),
        }
    }
}

/// Result of `forkReplaceResume`: the replacements applied (each with its dist report)
/// and the first one that failed; the ones after it stay pending.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ForkResume {
    pub applied: Vec<ReplacementPlan>,
    pub failed: Option<ResumeFailure>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct ResumeFailure {
    pub original: String,
    pub fork: String,
    /// Stable kind of the failure (as `PmError` kinds), with its message.
    pub error_kind: String,
    pub error_message: String,
}
impl From<pm::ForkResume> for ForkResume {
    fn from(r: pm::ForkResume) -> Self {
        Self {
            applied: r.applied.into_iter().map(Into::into).collect(),
            failed: r.failed.map(|f| ResumeFailure {
                error_kind: f.error.kind().to_owned(),
                error_message: f.error.to_string(),
                original: f.original,
                fork: f.fork,
            }),
        }
    }
}

/// A release-looking tag that was passed over (broken manifest, mismatch, …).
#[derive(Debug, Clone, uniffi::Record)]
pub struct SkippedTag {
    pub name: String,
    pub version: String,
    pub reason: String,
}
impl From<pm::releases::SkippedTag> for SkippedTag {
    fn from(t: pm::releases::SkippedTag) -> Self {
        Self {
            name: t.name,
            version: t.version,
            reason: t.reason,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct InstallResult {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub fork_of: Option<ForkOf>,
    /// Already installed from this source: nothing was changed, `current` is the
    /// installed revision.
    pub already_installed: bool,
    /// With `already_installed`: the revision the source resolved to (latest release, or
    /// the requested ref or default branch) when it differs from `current`; `update`
    /// with the same target moves to it.
    pub available: Option<Revision>,
    /// The recorded mode (with `already_installed`: the mode it keeps).
    pub mode: Mode,
    /// With `already_installed`: the requested mode when it differs from the recorded one.
    pub requested_mode: Option<Mode>,
    /// Broken release tags passed over while choosing the latest release; warn about them.
    pub skipped: Vec<SkippedTag>,
    /// Ignored wrong-typed metadata keys of the installed manifest; show them.
    pub warnings: Vec<String>,
    /// Recorded enabled: false after a download (with `already_installed` or a restore:
    /// the recorded flag).
    pub enabled: bool,
    /// Missing dependencies installed (or downloaded) from the catalog in the same
    /// transaction, sorted by ID.
    pub dependencies: Vec<InstalledDependency>,
    /// Reconciliation report of the transaction.
    pub dist: DistSync,
}
/// A dependency an install or download added from the catalog.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InstalledDependency {
    pub id: String,
    pub source: String,
    pub current: Revision,
    /// Recorded enabled (false when downloaded).
    pub enabled: bool,
    /// Ignored wrong-typed metadata keys of its manifest.
    pub warnings: Vec<String>,
}
impl From<pm::InstalledDependency> for InstalledDependency {
    fn from(d: pm::InstalledDependency) -> Self {
        Self {
            id: d.id,
            source: d.source,
            current: d.current.into(),
            enabled: d.enabled,
            warnings: d.warnings,
        }
    }
}
impl From<pm::InstallResult> for InstallResult {
    fn from(r: pm::InstallResult) -> Self {
        Self {
            id: r.id,
            source: r.source,
            current: r.current.into(),
            fork_of: r.fork_of.map(Into::into),
            already_installed: r.already_installed,
            available: r.available.map(Into::into),
            mode: r.mode.into(),
            requested_mode: r.requested_mode.map(Into::into),
            skipped: r.skipped.into_iter().map(Into::into).collect(),
            warnings: r.warnings,
            enabled: r.enabled,
            dependencies: r.dependencies.into_iter().map(Into::into).collect(),
            dist: r.dist.into(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RevisionChange {
    pub id: String,
    pub from: Revision,
    pub to: Revision,
    /// False when the package was already at the target revision.
    pub changed: bool,
    /// A breaking update: `to` is newer and outside the caret range of `from` (a new
    /// MAJOR; for 0.x a new MINOR). The CLI line ends with ` (major)`.
    pub major: bool,
    /// Ignored wrong-typed metadata keys of the new manifest.
    pub warnings: Vec<String>,
    /// Broken release tags passed over while choosing the latest release.
    pub skipped: Vec<SkippedTag>,
    /// Why an unchanged package stayed (for example pinned).
    pub unchanged_reason: Option<UnchangedReason>,
    /// The recorded mode afterwards; a change of mode alone is `changed` with `from` and
    /// `to` at the same commit.
    pub mode: Mode,
    /// The verified catalog the package was detached from (`update(…, detach: true)`).
    pub detached: Option<String>,
    /// Reconciliation report (empty inside an `UpdateAllPlan`, which carries its own).
    pub dist: DistSync,
}
impl From<pm::RevisionChange> for RevisionChange {
    fn from(c: pm::RevisionChange) -> Self {
        Self {
            id: c.id,
            from: c.from.into(),
            to: c.to.into(),
            changed: c.changed,
            major: c.major,
            warnings: c.warnings,
            skipped: c.skipped.into_iter().map(Into::into).collect(),
            unchanged_reason: c.unchanged_reason.map(Into::into),
            mode: c.mode.into(),
            detached: c.detached,
            dist: c.dist.into(),
        }
    }
}

/// Serializable joint upgrade result, also used for a read-only preview.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdateAllPlan {
    pub updates: Vec<RevisionChange>,
    pub installs: Vec<InstallResult>,
    pub kept_back: Vec<HeldBack>,
    /// Installed packages that stay as they are (nothing newer, or a `reason`).
    pub unchanged: Vec<Unchanged>,
    /// Broken release tags passed over for any installed package.
    pub skipped: Vec<SkippedTag>,
    /// Reconciliation report of `updateAll` (empty for the plan).
    pub dist: DistSync,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct Unchanged {
    pub id: String,
    pub version: String,
    /// Why it stays although it could move (pinned, diverged branch history, a broken
    /// dependency).
    pub reason: Option<UnchangedReason>,
    /// `reason` as the CLI prints it (`dependency acme/lib is not installed`).
    pub message: Option<String>,
    /// A moved or deleted installed release tag (the installed commit is kept).
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct HeldBack {
    pub id: String,
    pub selected: String,
    pub latest: String,
    /// Why `latest` was not selected.
    pub reason: HeldBackReason,
    /// `reason` as the CLI prints it (`dependency constraints`, `no longer replaces …`).
    pub message: String,
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
                    message: p.reason.to_string(),
                    reason: p.reason.into(),
                })
                .collect(),
            unchanged: plan
                .unchanged
                .into_iter()
                .map(|u| Unchanged {
                    id: u.id,
                    version: u.version,
                    message: u.reason.as_ref().map(ToString::to_string),
                    reason: u.reason.map(Into::into),
                    warnings: u.warnings,
                })
                .collect(),
            skipped: plan.skipped.into_iter().map(Into::into).collect(),
            dist: plan.dist.into(),
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

/// What `forget` did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ForgetResult {
    /// The clone stays with a `forgotten` record; false when it was gone (only the record
    /// and `dist/` were removed).
    pub clone_kept: bool,
    pub dist: DistSync,
}
impl From<pm::ForgetResult> for ForgetResult {
    fn from(r: pm::ForgetResult) -> Self {
        Self {
            clone_kept: r.clone_kept,
            dist: r.dist.into(),
        }
    }
}

/// What `syncDist` changes (or would change) so that `dist/` holds exactly the active
/// packages (installed, enabled, not replaced). IDs are sorted.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DistSync {
    /// Active packages whose missing `dist/<id>` is deployed.
    pub deployed: Vec<String>,
    /// Inactive packages whose `dist/<id>` is removed.
    pub removed: Vec<String>,
    /// Workspace-relative paths under `dist/` of no known package, or not real folders
    /// where a namespace folder belongs (left alone).
    pub unmanaged: Vec<String>,
    /// Active packages left without `dist/`, with the reason (clone missing, …).
    pub skipped: Vec<SkippedDist>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct SkippedDist {
    pub id: String,
    pub reason: String,
    /// The clone is gone: uninstall or forget drops the record.
    pub clone_missing: bool,
}
impl From<pm::DistSync> for DistSync {
    fn from(s: pm::DistSync) -> Self {
        Self {
            deployed: s.deployed,
            removed: s.removed,
            unmanaged: s.unmanaged,
            skipped: s
                .skipped
                .into_iter()
                .map(|s| SkippedDist {
                    id: s.id,
                    reason: s.reason,
                    clone_missing: s.clone_missing,
                })
                .collect(),
        }
    }
}
impl From<DistSync> for pm::DistSync {
    fn from(s: DistSync) -> Self {
        Self {
            deployed: s.deployed,
            removed: s.removed,
            unmanaged: s.unmanaged,
            skipped: s
                .skipped
                .into_iter()
                .map(|s| pm::SkippedDist {
                    id: s.id,
                    reason: s.reason,
                    clone_missing: s.clone_missing,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct UninstallPlan {
    pub id: String,
    /// False for a forgotten repository that is still on disk.
    pub registered: bool,
    pub deletes: Vec<String>,
    /// Empty: uninstall without asking. Otherwise show it and, once the user confirmed,
    /// pass exactly these items as `uninstall(id, confirmed)`; work that appeared since
    /// is refused.
    pub lost_work: Vec<LostWork>,
    /// Installed packages that would break: uninstall refuses unless `force`
    /// (`uninstallRefusal` gives that refusal without calling `uninstall`).
    pub dependents: Vec<String>,
    /// Uninstall would reactivate an original whose dependencies are not active: the
    /// `DependencyConflict` message (also thrown by `uninstallRefusal`).
    pub dependency_conflict: Option<String>,
}
impl From<UninstallPlan> for pm::UninstallPlan {
    fn from(p: UninstallPlan) -> Self {
        Self {
            id: p.id,
            registered: p.registered,
            deletes: p.deletes,
            lost_work: p.lost_work.into_iter().map(Into::into).collect(),
            dependents: p.dependents,
            dependency_conflict: p.dependency_conflict,
        }
    }
}

/// Throws the refusal `uninstall` raises for `plan` without force (`HasDependents` with
/// dependents and lost work, or `DependencyConflict`); returns normally when there is none.
#[uniffi::export]
pub fn uninstall_refusal(plan: UninstallPlan) -> Result<()> {
    match pm::UninstallPlan::from(plan).refusal() {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
impl From<pm::UninstallPlan> for UninstallPlan {
    fn from(p: pm::UninstallPlan) -> Self {
        Self {
            id: p.id,
            registered: p.registered,
            deletes: p.deletes,
            lost_work: p.lost_work.into_iter().map(Into::into).collect(),
            dependents: p.dependents,
            dependency_conflict: p.dependency_conflict,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum CheckStatus {
    DifferentSource,
    InvalidCatalogData,
    NoVersions,
    UncomparableVersion,
    SameVersionDifferentCommit,
    UpdateAvailable,
    UpToDate,
    NewerThanCatalog,
    OtherCatalog,
    NewerThanOrigin,
    NoReleases,
    OriginUnreachable,
    Pinned,
    UpstreamNotInCatalog,
}
impl From<pm::check::CheckStatus> for CheckStatus {
    fn from(s: pm::check::CheckStatus) -> Self {
        use pm::check::CheckStatus as S;
        match s {
            S::DifferentSource => CheckStatus::DifferentSource,
            S::InvalidCatalogData => CheckStatus::InvalidCatalogData,
            S::NoVersions => CheckStatus::NoVersions,
            S::UncomparableVersion => CheckStatus::UncomparableVersion,
            S::SameVersionDifferentCommit => CheckStatus::SameVersionDifferentCommit,
            S::UpdateAvailable => CheckStatus::UpdateAvailable,
            S::UpToDate => CheckStatus::UpToDate,
            S::NewerThanCatalog => CheckStatus::NewerThanCatalog,
            S::OtherCatalog => CheckStatus::OtherCatalog,
            S::NewerThanOrigin => CheckStatus::NewerThanOrigin,
            S::NoReleases => CheckStatus::NoReleases,
            S::OriginUnreachable => CheckStatus::OriginUnreachable,
            S::Pinned => CheckStatus::Pinned,
            S::UpstreamNotInCatalog => CheckStatus::UpstreamNotInCatalog,
        }
    }
}

/// Where a check row (or a fork's upstream) was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CheckSource {
    Catalog,
    Origin,
}
impl From<pm::check::CheckSource> for CheckSource {
    fn from(s: pm::check::CheckSource) -> Self {
        match s {
            pm::check::CheckSource::Catalog => CheckSource::Catalog,
            pm::check::CheckSource::Origin => CheckSource::Origin,
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
    /// Whether the row comes from the catalog or from the package's origin.
    pub source: CheckSource,
    /// The recorded mode: a pinned row names its ref, a branch row compares with the
    /// branch HEAD.
    pub mode: Mode,
    /// Why the origin could not be read (`OriginUnreachable`).
    pub problem: Option<String>,
    /// The origin's newer release the catalog lacks (`UpstreamNotInCatalog`).
    pub origin_latest: Option<String>,
    /// For a fork: where its original stands in the catalog or at its origin.
    pub upstream: Option<Upstream>,
    /// The available update is a major one (`check` shows `LATEST (major)`).
    pub major: bool,
    /// A newer pre-release that is not offered (pre-releases are opt-in).
    pub prerelease: Option<String>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct Upstream {
    pub id: String,
    /// Original version the fork started from.
    pub base: String,
    pub latest: Option<String>,
    /// The original has a newer release than the fork base.
    pub newer: bool,
    /// Where `latest` was looked up; `None`: neither catalog nor `forkOf.source`.
    pub source: Option<CheckSource>,
    /// Why `forkOf.source` could not be read.
    pub problem: Option<String>,
}
impl From<pm::check::CheckRow> for CheckRow {
    fn from(r: pm::check::CheckRow) -> Self {
        Self {
            status_message: r.message(),
            id: r.id,
            installed: r.installed,
            latest: r.latest,
            enabled: r.enabled,
            update_available: r.update_available,
            status: r.status.into(),
            source: r.source.into(),
            mode: r.mode.into(),
            problem: r.problem,
            origin_latest: r.origin_latest,
            major: r.major,
            prerelease: r.prerelease,
            upstream: r.upstream.map(|u| Upstream {
                id: u.id,
                base: u.base,
                latest: u.latest,
                newer: u.newer,
                source: u.source.map(Into::into),
                problem: u.problem,
            }),
        }
    }
}
/// What `check` reports: rows, replacement bindings and unsatisfied dependencies.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CheckReport {
    pub rows: Vec<CheckRow>,
    /// The given catalog is verified (`"verified": true`); false without a catalog.
    pub catalog_verified: bool,
    /// Original → fork, for every consumer.
    pub replacements: HashMap<String, String>,
    pub unsatisfied: Vec<UnsatisfiedDependency>,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct UnsatisfiedDependency {
    pub package: String,
    pub dependency: String,
    pub requirement: String,
    pub problem: Option<String>,
}
impl From<pm::check::CheckReport> for CheckReport {
    fn from(r: pm::check::CheckReport) -> Self {
        Self {
            rows: r.rows.into_iter().map(Into::into).collect(),
            catalog_verified: r.catalog_verified,
            replacements: r.replacements.into_iter().collect(),
            unsatisfied: r
                .unsatisfied
                .into_iter()
                .map(|u| UnsatisfiedDependency {
                    package: u.package,
                    dependency: u.dependency,
                    requirement: u.requirement,
                    problem: u.problem,
                })
                .collect(),
        }
    }
}

/// A package record of `state.json` as stored (see `InstalledPackage` for the resolved
/// view).
#[derive(Debug, Clone, uniffi::Record)]
pub struct PackageRecord {
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    pub mode: Mode,
    pub fork_of: Option<ForkOf>,
    /// Dependency ID → version requirement of the current revision.
    pub dependencies: HashMap<String, String>,
    pub replaces: Option<Replaces>,
    pub catalog: Option<String>,
    pub clone: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub provides: Vec<String>,
    pub tags: Vec<String>,
}
impl From<pm::Installed> for PackageRecord {
    fn from(p: pm::Installed) -> Self {
        Self {
            source: p.source,
            current: p.current.into(),
            previous: p.previous.map(Into::into),
            enabled: p.enabled,
            mode: p.mode.into(),
            fork_of: p.fork_of.map(Into::into),
            dependencies: p.dependencies.into_iter().collect(),
            replaces: p.replaces.map(Into::into),
            catalog: p.catalog,
            clone: p.clone,
            path: p.path,
            title: p.metadata.title,
            description: p.metadata.description,
            provides: p.metadata.provides,
            tags: p.metadata.tags,
        }
    }
}
/// The state view of `list`/`readState`: raw `state.json` plus `resolved`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ListView {
    pub format: u32,
    pub packages: HashMap<String, PackageRecord>,
    /// Original → fork, for every consumer.
    pub replacements: HashMap<String, String>,
    /// Forgotten packages, own or shared clone (restorable as recorded).
    pub forgotten: HashMap<String, PackageRecord>,
    pub resolved: Vec<InstalledPackage>,
}
impl From<pm::ListView> for ListView {
    fn from(v: pm::ListView) -> Self {
        Self {
            format: v.state.format,
            packages: v
                .state
                .packages
                .into_iter()
                .map(|(id, p)| (id, p.into()))
                .collect(),
            replacements: v.state.replacements.into_iter().collect(),
            forgotten: v
                .state
                .forgotten
                .into_iter()
                .map(|(id, p)| (id, p.into()))
                .collect(),
            resolved: v.resolved.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PackageSummary {
    pub id: String,
    pub source: String,
    pub versions: Vec<String>,
    /// The newest version that is not a pre-release (what `install-catalog` without a
    /// version installs); `None` when there is none.
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
    Package { depth: u32, package: PackageSummary },
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct CatalogOverview {
    pub location: String,
    pub remote: bool,
    /// The catalog's top-level `catalog.json` has `"verified": true`.
    pub verified: bool,
    /// Depth-first rows of the catalog tree (flat; use `depth` for indentation).
    pub items: Vec<OverviewItem>,
    pub truncated: bool,
}
impl From<pm::catalog::CatalogOverview> for CatalogOverview {
    fn from(o: pm::catalog::CatalogOverview) -> Self {
        Self {
            location: o.location,
            remote: o.remote,
            verified: o.verified,
            items: o
                .items
                .into_iter()
                .map(|item| match item {
                    pm::catalog::OverviewItem::Directory { depth, name } => {
                        OverviewItem::Directory { depth, name }
                    }
                    pm::catalog::OverviewItem::Empty { depth } => OverviewItem::Empty { depth },
                    pm::catalog::OverviewItem::Package { depth, package } => {
                        OverviewItem::Package {
                            depth,
                            package: PackageSummary {
                                id: package.id,
                                source: package.source,
                                versions: package.versions,
                                latest: package.latest,
                                title: package.title,
                                description: package.description,
                                provides: package.provides,
                                tags: package.tags,
                            },
                        }
                    }
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
    /// The replace step needs confirmation (`failedStep` replace): the plan to show;
    /// `forkReplaceResume` applies it.
    pub pending: Option<ReplacementPlan>,
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
/// Result of `PackageManager.forkPackages` (`gitpm::PackageFork`): `created` is false
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
/// Result of `PackageManager.forkPackagesAndReplace` (`gitpm::PackageForkReplacement`):
/// one entry per package (install order), each with its own
/// `failedStep`/`errorKind`/`errorMessage`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PackageForkReplacement {
    pub fork: PackageFork,
    pub packages: Vec<ForkReplacement>,
    /// The replacements waiting for confirmation (for `forkReplaceResume`).
    pub pending: Vec<PendingReplacement>,
}
impl From<pm::PackageForkReplacement> for PackageForkReplacement {
    fn from(r: pm::PackageForkReplacement) -> Self {
        Self {
            pending: r.pending().into_iter().map(Into::into).collect(),
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
            pending: r.pending.map(Into::into),
        }
    }
}

/// What `update` moves a package to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Target {
    /// Back to release mode at the newest release.
    Release,
    /// A tag or commit (pinned) or an origin branch (followed); the mode is recorded.
    Ref { reference: String },
    /// Follow the origin's default branch.
    Dev,
    /// Follow the recorded mode (a pinned package stays, reason `pinned`).
    Recorded,
}
impl From<Target> for pm::Target {
    fn from(target: Target) -> Self {
        match target {
            Target::Release => pm::Target::Release,
            Target::Ref { reference } => pm::Target::Ref(reference),
            Target::Dev => pm::Target::Dev,
            Target::Recorded => pm::Target::Recorded,
        }
    }
}
/// What `install` installs: `Target` without `Recorded` (nothing is recorded yet).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum InstallTarget {
    Release,
    Ref { reference: String },
    Dev,
}
impl From<InstallTarget> for pm::InstallTarget {
    fn from(target: InstallTarget) -> Self {
        match target {
            InstallTarget::Release => pm::InstallTarget::Release,
            InstallTarget::Ref { reference } => pm::InstallTarget::Ref(reference),
            InstallTarget::Dev => pm::InstallTarget::Dev,
        }
    }
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct ManagerOptions {
    /// Catalog location: local folder, JSON file or HTTP(S) URL.
    #[uniffi(default = None)]
    pub catalog: Option<String>,
    /// Preserve the host's SSH transport for all workspace Git commands.
    #[uniffi(default = false)]
    pub custom_ssh: bool,
}

impl From<ManagerOptions> for pm::Options {
    fn from(options: ManagerOptions) -> Self {
        Self {
            catalog: options.catalog,
            custom_ssh: options.custom_ssh,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Manager object

/// An open, locked workspace. Thread-safe; operations are serialized internally.
#[derive(uniffi::Object)]
pub struct PackageManager {
    inner: pm::Manager,
}

#[uniffi::export]
impl PackageManager {
    /// Open (or create) a dedicated workspace directory and take its lock.
    #[uniffi::constructor]
    pub fn open(root: String, options: ManagerOptions) -> Result<Arc<Self>> {
        let inner = pm::Manager::open_with(root, options.into())?;
        Ok(Arc::new(Self { inner }))
    }
    pub fn root(&self) -> String {
        self.inner.root().to_string_lossy().into_owned()
    }
    pub fn list(&self) -> Result<Vec<InstalledPackage>> {
        Ok(self.inner.list()?.into_iter().map(Into::into).collect())
    }
    pub fn browse_catalog(&self) -> Result<CatalogOverview> {
        Ok(self.inner.browse_catalog()?.into())
    }
    /// Release from the configured catalog; latest if `version` is `None`.
    pub fn catalog_release(&self, id: String, version: Option<String>) -> Result<CatalogRelease> {
        Ok(self.inner.catalog_release(&id, version.as_deref())?.into())
    }
    /// Install `target` from a Git source (URL or local path) and record its mode.
    /// `package` picks a package of a repository with `gitpm.json` (needed when it lists
    /// several, unless the ref is an `ID@VERSION` tag); for a single-package repository
    /// it must match. A dependency provided only by a disabled package refuses with
    /// `DependencyConflict` unless `force` (which enables only this package); over a
    /// download from the same source the install enables it.
    #[uniffi::method(default(package = None, force = false))]
    pub fn install(
        &self,
        source: String,
        target: InstallTarget,
        package: Option<String>,
        force: bool,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .install_package_with(&source, package.as_deref(), target.into(), force)?
            .into())
    }
    /// Install an exact catalog release (latest if `version` is `None`), verifying hashes;
    /// `force` as for `install`.
    #[uniffi::method(default(force = false))]
    pub fn install_from_catalog(
        &self,
        id: String,
        version: Option<String>,
        force: bool,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .install_from_catalog_with(&id, version.as_deref(), force)?
            .into())
    }
    /// Like `install`, but the package and the dependencies it pulls are recorded
    /// disabled and nothing is deployed; `enable` deploys them later (offline).
    #[uniffi::method(default(package = None))]
    pub fn download(
        &self,
        source: String,
        target: InstallTarget,
        package: Option<String>,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .download(&source, package.as_deref(), target.into())?
            .into())
    }
    /// Like `installFromCatalog`, recorded disabled and not deployed.
    pub fn download_from_catalog(
        &self,
        id: String,
        version: Option<String>,
    ) -> Result<InstallResult> {
        Ok(self
            .inner
            .download_from_catalog(&id, version.as_deref())?
            .into())
    }
    /// Move `id` by `target`; `Recorded` follows the recorded mode. A package protected
    /// by a verified catalog moves only to the versions it lists (hashes checked);
    /// `detach` drops that protection and updates from the origin.
    #[uniffi::method(default(detach = false))]
    pub fn update(&self, id: String, target: Target, detach: bool) -> Result<RevisionChange> {
        Ok(self
            .inner
            .update_with(&id, target.into(), pm::UpdateOptions { detach })?
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
    /// Deploys `dist/<id>` again (unless the package is replaced). Refuses with
    /// `DependencyConflict` while a dependency is not satisfied by an active package,
    /// unless `force`. Returns the dist report.
    #[uniffi::method(default(force = false))]
    pub fn enable(&self, id: String, force: bool) -> Result<DistSync> {
        Ok(self.inner.enable_with(&id, force)?.into())
    }
    /// Removes `dist/<id>`; a replacing package also drops its replacement (the original
    /// is deployed again when active). Refuses with `HasDependents` when consumers would
    /// break, and with `DependencyConflict` when a reactivated original has inactive
    /// dependencies, unless `force`.
    #[uniffi::method(default(force = false))]
    pub fn disable(&self, id: String, force: bool) -> Result<DistSync> {
        Ok(self.inner.set_enabled_with(&id, false, force)?.into())
    }
    /// Unregister; the repository stays on disk, `dist/<id>` is removed (`restore`
    /// deploys it again). Dependents refuse unless `force`. A package whose clone is gone
    /// keeps no record (`cloneKept` false).
    #[uniffi::method(default(force = false))]
    pub fn forget(&self, id: String, force: bool) -> Result<ForgetResult> {
        Ok(self.inner.forget_with(&id, force)?.into())
    }
    /// Record a forgotten package again as it was recorded (revisions, catalog, enabled
    /// flag, mode).
    pub fn restore(&self, id: String) -> Result<InstallResult> {
        Ok(self.inner.restore(&id)?.into())
    }
    /// Read-only: what `syncDist` would change.
    pub fn sync_dist_plan(&self) -> Result<DistSync> {
        Ok(self.inner.sync_dist_plan()?.into())
    }
    /// Make `dist/` hold exactly the active packages (an explicit repair, for example after
    /// a folder was copied into `dist/` by hand). Mutating calls reconcile too and return
    /// their dist report. Active packages left undeployed: `NotDeployed` (what could be
    /// synced is committed).
    pub fn sync_dist(&self) -> Result<DistSync> {
        Ok(self.inner.sync_dist()?.into())
    }
    pub fn uninstall_plan(&self, id: String) -> Result<UninstallPlan> {
        Ok(self.inner.uninstall_plan(&id)?.into())
    }
    /// Local work not in `confirmed` (the plan's `lostWork` the user saw) refuses with
    /// `LocalChanges` listing it; dependents that would break refuse with
    /// `HasDependents` unless `force`. A package whose clone is gone is removed with
    /// nothing confirmed.
    #[uniffi::method(default(force = false))]
    pub fn uninstall(&self, id: String, confirmed: Vec<LostWork>, force: bool) -> Result<DistSync> {
        let confirmed: Vec<pm::LostWork> = confirmed.into_iter().map(Into::into).collect();
        Ok(self.inner.uninstall(&id, &confirmed, force)?.into())
    }
    /// Read-only: what replacing `original` by `replacement` would change.
    pub fn replace_plan(&self, original: String, replacement: String) -> Result<ReplacementPlan> {
        Ok(self.inner.replace_plan(&original, &replacement)?.into())
    }
    /// Make `replacement` the provider of `original` for every consumer. Unsatisfied
    /// consumers refuse with `IncompatibleReplacement` unless `confirm`. Removes
    /// `dist/<original>`; the replacement keeps its own `dist/`.
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
    /// Every consumer uses the installed original again (same confirmation rule);
    /// `dist/<original>` is deployed again. Consumers the original does not serve (not
    /// installed, disabled) refuse with `HasDependents` unless `force`.
    #[uniffi::method(default(force = false))]
    pub fn switch_back(
        &self,
        original: String,
        confirm: bool,
        force: bool,
    ) -> Result<ReplacementPlan> {
        Ok(self.inner.switch_back(&original, confirm, force)?.into())
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
    /// Apply the pending replacements of `forkPackagesAndReplace` (or `forkAndReplace`)
    /// after the user confirmed them (`confirm`), in order. Each is planned again; when its
    /// incompatible consumers changed it is refused (`incompatible_replacement`). The
    /// first failure stops the run and is reported with what was applied before it.
    pub fn fork_replace_resume(
        &self,
        pending: Vec<PendingReplacement>,
        confirm: bool,
    ) -> ForkResume {
        let pending: Vec<pm::PendingReplacement> = pending.into_iter().map(Into::into).collect();
        self.inner.fork_replace_resume(&pending, confirm).into()
    }
}

// ---------------------------------------------------------------------------------------
// Free functions (no workspace needed)

/// Check read access to a Git repository with the user's credentials; nothing is cloned.
/// `custom_ssh`: the process environment already sets GIT_SSH_COMMAND/GIT_SSH.
#[uniffi::export]
pub fn auth_check(source: String, custom_ssh: bool) -> Result<AuthCheck> {
    let result = pm::auth::check(
        &source,
        &pm::Options {
            custom_ssh,
            ..Default::default()
        },
    )?;
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

/// The list view (what `gitpm list [--provides SERVICE]` prints), read without
/// opening, locking or creating the workspace.
#[uniffi::export]
pub fn list(root: String, provides: Option<String>) -> Result<ListView> {
    let options = pm::ListOptions { provides };
    Ok(pm::list_with(std::path::Path::new(&root), &options)?.into())
}

/// The full state view (raw state including `forgotten` and `replacements`, plus
/// `resolved` with `active`), read without opening the workspace.
#[uniffi::export]
pub fn read_state(root: String) -> Result<ListView> {
    list(root, None)
}

/// Compare the installed versions with a catalog (what `gitpm check` prints), without
/// opening the workspace. An empty workspace needs no catalog. `custom_ssh` is the
/// same setting as `ManagerOptions.customSsh`.
#[uniffi::export]
pub fn check(root: String, catalog: Option<String>, custom_ssh: bool) -> Result<CheckReport> {
    let options = pm::Options {
        custom_ssh,
        ..Default::default()
    };
    Ok(
        pm::check::check_with_options(std::path::Path::new(&root), catalog.as_deref(), &options)?
            .into(),
    )
}

/// The directory links at one catalog level (what `gitpm catalog` prints).
#[derive(Debug, Clone, uniffi::Record)]
pub struct CatalogLinks {
    pub schema: u32,
    pub directories: HashMap<String, String>,
}
#[uniffi::export]
pub fn catalog_links(location: String) -> Result<CatalogLinks> {
    let links = pm::catalog::links(&location)?;
    Ok(CatalogLinks {
        schema: links.schema,
        directories: links.directories.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_keep_kind_and_message() {
        let error: PmError = pm::Error::NotInstalled { id: "a/b".into() }.into();
        assert!(matches!(&error, PmError::NotInstalled { id, .. } if id == "a/b"));
        assert_eq!(error.to_string(), "package a/b is not installed");
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
            PackageManager::open(root.to_string_lossy().into(), ManagerOptions::default()).unwrap();
        assert!(manager.list().unwrap().is_empty());
        assert!(matches!(
            manager.uninstall_plan("a/b".into()),
            Err(PmError::RepositoryNotFound { .. })
        ));
        let report = check(root.to_string_lossy().into(), None, false).unwrap();
        assert!(report.rows.is_empty() && report.unsatisfied.is_empty());
        assert!(matches!(
            PackageManager::open(root.to_string_lossy().into(), ManagerOptions::default()),
            Err(PmError::Locked { .. })
        ));
        assert!(manager.update_all_plan().unwrap().updates.is_empty());
        assert!(manager.update_all().unwrap().installs.is_empty());
        assert!(matches!(
            manager.enable("a/b".into(), true),
            Err(PmError::NotInstalled { .. })
        ));
        assert!(matches!(
            manager.download_from_catalog("a/b".into(), None),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.install_from_catalog("a/b".into(), None, true),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(manager.list().unwrap().is_empty());
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
            pm::Error::InvalidManifest {
                id: None,
                message: m(),
            },
            pm::Error::InvalidVersion(m()),
            pm::Error::InvalidArgument(m()),
            pm::Error::VersionConflict {
                id: None,
                message: m(),
            },
            pm::Error::NoPreviousVersion {
                id: "a/b".into(),
                commit: Some("c0ffee".into()),
            },
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
                lost_work: vec![],
            },
            pm::Error::IncompatibleReplacement {
                id: "x/one".into(),
                message: m(),
                consumers: vec!["app/b".into()],
            },
            pm::Error::NotDeployed {
                message: m(),
                sync: Box::new(pm::DistSync {
                    skipped: vec![pm::SkippedDist {
                        id: "x/one".into(),
                        reason: "clone missing".into(),
                        clone_missing: true,
                    }],
                    ..Default::default()
                }),
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
            PmError::NotDeployed { .. } => "not_deployed",
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
            | PmError::InvalidManifest { message, .. }
            | PmError::InvalidVersion { message }
            | PmError::InvalidArgument { message }
            | PmError::VersionConflict { message, .. }
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
            | PmError::NotDeployed { message, .. }
            | PmError::InvalidData { message }
            | PmError::Io { message }
            | PmError::Other { message, .. } => message,
        }
    }

    #[test]
    fn every_library_error_maps_to_its_own_case_with_the_full_message() {
        let errors = library_errors();
        assert_eq!(errors.len(), 24);
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
            | PmError::RepositoryNotFound { id, .. }
            | PmError::InvalidManifest { id: Some(id), .. }
            | PmError::VersionConflict { id: Some(id), .. } => id,
            other => panic!("no id: {other:?}"),
        };
        for error in [
            pm::Error::InvalidManifest {
                id: Some("x/one".into()),
                message: "x/one: rollback manifest mismatch".into(),
            },
            pm::Error::VersionConflict {
                id: Some("x/one".into()),
                message: "x/one: history of main diverged; automatic update refused".into(),
            },
            pm::Error::NotInstalled { id: "x/one".into() },
            pm::Error::AlreadyInstalled {
                id: "x/one".into(),
                message: "m".into(),
            },
            pm::Error::NoPreviousVersion {
                id: "x/one".into(),
                commit: Some("c0ffee".into()),
            },
            pm::Error::RepositoryNotFound { id: "x/one".into() },
        ] {
            let message = error.to_string();
            let mapped = PmError::from(error);
            assert_eq!(mapped.to_string(), message);
            assert_eq!(id_of(mapped), "x/one");
        }
        for error in [
            pm::Error::InvalidManifest {
                id: None,
                message: "global manifest error".into(),
            },
            pm::Error::VersionConflict {
                id: None,
                message: "global version conflict".into(),
            },
        ] {
            let message = error.to_string();
            let mapped = PmError::from(error);
            assert_eq!(mapped.to_string(), message);
            assert!(matches!(
                mapped,
                PmError::InvalidManifest { id: None, .. }
                    | PmError::VersionConflict { id: None, .. }
            ));
        }
        let PmError::NoPreviousVersion { commit, .. } =
            PmError::from(pm::Error::NoPreviousVersion {
                id: "x/one".into(),
                commit: Some("c0ffee".into()),
            })
        else {
            unreachable!()
        };
        assert_eq!(commit.as_deref(), Some("c0ffee"));
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
        // The dependents refusal of uninstall carries the lost work too.
        let error = PmError::from(pm::Error::HasDependents {
            id: "x/one".into(),
            message: "m".into(),
            dependents: vec!["app/a".into()],
            lost_work: items.clone(),
        });
        let PmError::HasDependents {
            id,
            dependents,
            lost_work,
            ..
        } = error
        else {
            panic!()
        };
        assert_eq!(
            (id.as_str(), dependents.as_slice()),
            ("x/one", ["app/a".to_owned()].as_slice())
        );
        let back: Vec<pm::LostWork> = lost_work.into_iter().map(Into::into).collect();
        assert_eq!(back, items);
        // uninstallRefusal throws exactly the library's refusal of the plan.
        let plan = pm::UninstallPlan {
            id: "x/one".into(),
            registered: true,
            deletes: vec![],
            lost_work: items.clone(),
            dependents: vec!["app/a".into()],
            dependency_conflict: None,
        };
        let expected = plan.refusal().unwrap().to_string();
        match uninstall_refusal(plan.clone().into()) {
            Err(PmError::HasDependents {
                message, lost_work, ..
            }) => {
                assert_eq!(message, expected);
                assert_eq!(lost_work.len(), items.len());
            }
            other => panic!("{other:?}"),
        }
        let clean = pm::UninstallPlan {
            dependents: vec![],
            ..plan
        };
        assert!(uninstall_refusal(clean.clone().into()).is_ok());
        let conflict = pm::UninstallPlan {
            dependency_conflict: Some("m".into()),
            ..clean
        };
        assert!(matches!(
            uninstall_refusal(conflict.into()),
            Err(PmError::DependencyConflict { message }) if message == "m"
        ));
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
            mode: pm::Mode::Release,
        }
    }
    fn fork_of() -> pm::ForkOf {
        pm::ForkOf {
            id: "vendor/x".into(),
            version: "0.9.0".into(),
            commit: "f".repeat(40),
            source: Some("https://git.example/vendor/x.git".into()),
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
            pending: None,
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
        let options: pm::Options = ManagerOptions {
            catalog: Some("catalog.json".into()),
            custom_ssh: true,
        }
        .into();
        assert_eq!(options.catalog.as_deref(), Some("catalog.json"));
        assert!(options.custom_ssh);
        assert!(!pm::Options::from(ManagerOptions::default()).custom_ssh);
        let package = InstalledPackage::from(pm::InstalledPackage {
            id: "me/x".into(),
            source: "src".into(),
            current: revision("1.0.0"),
            previous: Some(revision("0.9.0")),
            enabled: false,
            mode: pm::Mode::Branch("main".into()),
            active: false,
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
            (
                package.id.as_str(),
                package.source.as_str(),
                package.enabled
            ),
            ("me/x", "src", false)
        );
        assert_eq!(
            package.mode,
            Mode::Branch {
                name: "main".into()
            }
        );
        assert_eq!(package.path.as_deref(), Some("packages/x"));
        assert_eq!(package.clone.as_deref(), Some("mono-0123456789abcdef"));
        assert_eq!(
            (package.title.as_deref(), package.description.as_deref()),
            (Some("X"), None)
        );
        assert_eq!(package.provides, ["theme"]);
        assert_eq!(package.tags, ["ui", "dark"]);
        assert_eq!(
            (
                package.current.version.as_str(),
                package.current.commit.as_str()
            ),
            ("1.0.0", "1.0.0-commit")
        );
        assert_eq!(package.previous.unwrap().version, "0.9.0");
        let f = package.fork_of.unwrap();
        assert_eq!(
            (f.id.as_str(), f.version.as_str(), f.commit),
            ("vendor/x", "0.9.0", "f".repeat(40))
        );
        assert_eq!(
            f.source.as_deref(),
            Some("https://git.example/vendor/x.git")
        );

        let check = CheckReport::from(pm::check::CheckReport {
            catalog_verified: true,
            rows: vec![pm::check::CheckRow {
                id: "me/x".into(),
                installed: "1.0.0".into(),
                latest: None,
                enabled: true,
                update_available: false,
                status: pm::check::CheckStatus::OriginUnreachable,
                source: pm::check::CheckSource::Origin,
                mode: pm::Mode::Pinned("me/x@1.0.0".into()),
                problem: Some("unreachable".into()),
                origin_latest: Some("1.2.0".into()),
                major: true,
                prerelease: Some("2.1.0-rc.1".into()),
                upstream: Some(pm::check::Upstream {
                    id: "vendor/x".into(),
                    base: "0.9.0".into(),
                    latest: Some("1.0.0".into()),
                    newer: true,
                    source: Some(pm::check::CheckSource::Origin),
                    problem: Some("original gone".into()),
                }),
            }],
            replacements: [("vendor/x".to_owned(), "me/x".to_owned())].into(),
            unsatisfied: vec![pm::check::UnsatisfiedDependency {
                package: "me/x".into(),
                dependency: "lib/core".into(),
                requirement: "^2".into(),
                problem: Some("not installed".into()),
            }],
        });
        assert!(check.catalog_verified);
        let row = &check.rows[0];
        assert_eq!(row.origin_latest.as_deref(), Some("1.2.0"));
        assert!(row.major);
        assert_eq!(row.prerelease.as_deref(), Some("2.1.0-rc.1"));
        assert_eq!(
            (
                row.id.as_str(),
                row.installed.as_str(),
                row.latest.as_deref(),
                row.enabled,
                row.update_available
            ),
            ("me/x", "1.0.0", None, true, false)
        );
        assert!(matches!(row.status, CheckStatus::OriginUnreachable));
        assert_eq!(row.status_message, "origin unreachable");
        assert_eq!(
            row.mode,
            Mode::Pinned {
                reference: "me/x@1.0.0".into()
            }
        );
        assert_eq!(
            (row.source, row.problem.as_deref()),
            (CheckSource::Origin, Some("unreachable"))
        );
        let upstream = row.upstream.as_ref().unwrap();
        assert_eq!(
            (
                upstream.id.as_str(),
                upstream.base.as_str(),
                upstream.latest.as_deref(),
                upstream.newer,
                upstream.source
            ),
            (
                "vendor/x",
                "0.9.0",
                Some("1.0.0"),
                true,
                Some(CheckSource::Origin)
            )
        );
        assert_eq!(check.replacements["vendor/x"], "me/x");
        let dep = &check.unsatisfied[0];
        assert_eq!(
            (
                dep.package.as_str(),
                dep.dependency.as_str(),
                dep.requirement.as_str(),
                dep.problem.as_deref()
            ),
            ("me/x", "lib/core", "^2", Some("not installed"))
        );

        let skipped = || {
            vec![pm::releases::SkippedTag {
                name: "me/x@0.5.0".into(),
                version: "0.5.0".into(),
                reason: "invalid manifest".into(),
            }]
        };
        let report = || pm::DistSync {
            deployed: vec!["me/x".into()],
            removed: vec!["vendor/x".into()],
            unmanaged: vec!["dist/custom".into()],
            skipped: vec![pm::SkippedDist {
                id: "me/y".into(),
                reason: "clone missing".into(),
                clone_missing: true,
            }],
        };
        let check_report = |dist: &DistSync| {
            assert_eq!(
                (dist.deployed.as_slice(), dist.removed.as_slice()),
                (
                    ["me/x".to_owned()].as_slice(),
                    ["vendor/x".to_owned()].as_slice()
                )
            );
            assert_eq!(dist.unmanaged, ["dist/custom"]);
            assert_eq!(
                (dist.skipped[0].id.as_str(), dist.skipped[0].reason.as_str()),
                ("me/y", "clone missing")
            );
            assert!(dist.skipped[0].clone_missing);
        };
        let result = InstallResult::from(pm::InstallResult {
            id: "me/x".into(),
            source: "src".into(),
            current: revision("1.0.0"),
            fork_of: None,
            skipped: skipped(),
            warnings: vec!["me/x 1.0.0: plugin.json title must be a string; ignored".into()],
            already_installed: true,
            available: Some(revision("1.1.0")),
            mode: pm::Mode::Branch("main".into()),
            requested_mode: Some(pm::Mode::Release),
            enabled: false,
            dependencies: vec![pm::InstalledDependency {
                id: "me/lib".into(),
                source: "lib-src".into(),
                current: revision("2.0.0"),
                enabled: false,
                warnings: vec!["lib warning".into()],
            }],
            dist: report(),
        });
        assert!(result.fork_of.is_none());
        assert!(!result.enabled);
        let dependency = &result.dependencies[0];
        assert_eq!(
            (
                dependency.id.as_str(),
                dependency.source.as_str(),
                dependency.current.version.as_str(),
                dependency.current.commit.as_str(),
                dependency.enabled,
                dependency.warnings.as_slice()
            ),
            (
                "me/lib",
                "lib-src",
                "2.0.0",
                "2.0.0-commit",
                false,
                ["lib warning".to_owned()].as_slice()
            )
        );
        assert_eq!(
            (result.mode.clone(), result.requested_mode.clone()),
            (
                Mode::Branch {
                    name: "main".into()
                },
                Some(Mode::Release)
            )
        );
        assert!(result.already_installed);
        assert_eq!(result.available.unwrap().version, "1.1.0");
        assert_eq!(result.current.commit, "1.0.0-commit");
        assert_eq!(
            (
                result.skipped[0].name.as_str(),
                result.skipped[0].version.as_str(),
                result.skipped[0].reason.as_str()
            ),
            ("me/x@0.5.0", "0.5.0", "invalid manifest")
        );
        assert_eq!(
            result.warnings,
            ["me/x 1.0.0: plugin.json title must be a string; ignored"]
        );
        check_report(&result.dist);

        let change = RevisionChange::from(pm::RevisionChange {
            id: "me/x".into(),
            from: revision("1.0.0"),
            to: revision("2.0.0"),
            changed: true,
            major: true,
            warnings: vec!["w".into()],
            skipped: skipped(),
            unchanged_reason: Some(pm::UnchangedReason::Pinned {
                reference: "me/x@1.0.0".into(),
            }),
            mode: pm::Mode::Pinned("me/x@1.0.0".into()),
            detached: Some("/catalogs/a".into()),
            dist: report(),
        });
        assert!(change.changed && change.major);
        assert_eq!(change.detached.as_deref(), Some("/catalogs/a"));
        assert_eq!(
            UnchangedReason::from(pm::UnchangedReason::NotInCatalog {
                version: "1.2.0".into()
            }),
            UnchangedReason::NotInCatalog {
                version: "1.2.0".into()
            }
        );
        assert_eq!(
            UnchangedReason::from(pm::UnchangedReason::CatalogUnavailable {
                reason: "gone".into()
            }),
            UnchangedReason::CatalogUnavailable {
                reason: "gone".into()
            }
        );
        assert_eq!(
            change.mode,
            Mode::Pinned {
                reference: "me/x@1.0.0".into()
            }
        );
        assert_eq!(
            change.unchanged_reason,
            Some(UnchangedReason::Pinned {
                reference: "me/x@1.0.0".into()
            })
        );
        assert_eq!(
            (change.from.version.as_str(), change.to.version.as_str()),
            ("1.0.0", "2.0.0")
        );
        assert_eq!(change.warnings, ["w"]);
        assert_eq!(change.skipped[0].name, "me/x@0.5.0");
        check_report(&change.dist);

        let plan = UpdateAllPlan::from(pm::UpdateAllPlan {
            updates: vec![],
            installs: vec![],
            kept_back: vec![pm::HeldBack {
                id: "me/fork".into(),
                selected: "2.0.0".into(),
                latest: "3.0.0".into(),
                reason: pm::HeldBackReason::NoLongerReplaces {
                    original: "vendor/base".into(),
                },
            }],
            unchanged: vec![
                pm::Unchanged {
                    id: "app/b".into(),
                    version: "2.0.0".into(),
                    reason: Some(pm::UnchangedReason::Diverged {
                        branch: "main".into(),
                    }),
                    warnings: vec![],
                },
                pm::Unchanged {
                    id: "app/c".into(),
                    version: "1.0.0".into(),
                    reason: Some(pm::UnchangedReason::BrokenDependency {
                        dependency: "acme/lib".into(),
                        problem: "not installed".into(),
                    }),
                    warnings: vec![],
                },
                pm::Unchanged {
                    id: "app/d".into(),
                    version: "1.0.0".into(),
                    reason: None,
                    warnings: vec![
                        "installed release app/d@1.0.0 no longer exists on the origin".into(),
                    ],
                },
            ],
            skipped: skipped(),
            dist: report(),
        });
        assert_eq!(
            (
                plan.unchanged[0].id.as_str(),
                plan.unchanged[0].version.as_str()
            ),
            ("app/b", "2.0.0")
        );
        assert_eq!(
            plan.unchanged[0].reason,
            Some(UnchangedReason::Diverged {
                branch: "main".into()
            })
        );
        let held = &plan.kept_back[0];
        assert_eq!(
            (
                held.id.as_str(),
                held.selected.as_str(),
                held.latest.as_str()
            ),
            ("me/fork", "2.0.0", "3.0.0")
        );
        assert_eq!(
            held.reason,
            HeldBackReason::NoLongerReplaces {
                original: "vendor/base".into()
            }
        );
        assert_eq!(held.message, "no longer replaces vendor/base");
        assert_eq!(
            plan.unchanged[1].reason,
            Some(UnchangedReason::BrokenDependency {
                dependency: "acme/lib".into(),
                problem: "not installed".into()
            })
        );
        assert_eq!(
            plan.unchanged[1].message.as_deref(),
            Some("dependency acme/lib is not installed")
        );
        assert_eq!(
            plan.unchanged[2].warnings,
            ["installed release app/d@1.0.0 no longer exists on the origin"]
        );
        assert_eq!(plan.unchanged[2].reason, None);
        check_report(&plan.dist);
        assert_eq!(plan.skipped[0].name, "me/x@0.5.0");

        let replacement = pm::ReplacementPlan {
            original: "vendor/x".into(),
            provider: "me/x".into(),
            version: "1.4.0".into(),
            current: None,
            consumers: vec!["app/a".into()],
            incompatible: vec![pm::IncompatibleConsumer {
                id: "app/a".into(),
                requirement: "^2".into(),
            }],
            installed_version: Some("0.3.0".into()),
            dist: report(),
        };
        let ffi = ReplacementPlan::from(replacement.clone());
        assert_eq!(ffi.installed_version.as_deref(), Some("0.3.0"));
        assert_eq!(ffi.incompatible[0].requirement, "^2");
        check_report(&ffi.dist);
        // Back to the library for forkReplaceResume: nothing is lost.
        let back = pm::ReplacementPlan::from(ffi.clone());
        assert_eq!(format!("{back:?}"), format!("{replacement:?}"));
        let pending = PendingReplacement::from(pm::PendingReplacement {
            original: "vendor/x".into(),
            fork: "me/x".into(),
            plan: replacement.clone(),
        });
        assert_eq!(
            (pending.original.as_str(), pending.fork.as_str()),
            ("vendor/x", "me/x")
        );
        let fork = ForkReplacement::from(pm::ForkReplacement {
            original: "vendor/x".into(),
            id: "me/x".into(),
            path: "/forks/me/x".into(),
            tag: Some("me/x@0.3.0".into()),
            installed: None,
            replacement: None,
            failed: Some(pm::ForkStep::Replace),
            error: None,
            pending: Some(replacement),
        });
        assert_eq!(fork.pending.unwrap().provider, "me/x");

        let error = PmError::from(pm::Error::NotDeployed {
            message: "1 package(s) not deployed: me/y".into(),
            sync: Box::new(report()),
        });
        let PmError::NotDeployed { message, sync } = error else {
            panic!()
        };
        assert_eq!(message, "1 package(s) not deployed: me/y");
        check_report(&sync);
        let resume = ForkResume::from(pm::ForkResume {
            applied: vec![pm::ReplacementPlan {
                dist: report(),
                ..back.clone()
            }],
            failed: Some(pm::ResumeFailure {
                original: "vendor/y".into(),
                fork: "me/y".into(),
                error: pm::Error::IncompatibleReplacement {
                    id: "vendor/y".into(),
                    message: "changed".into(),
                    consumers: vec!["app/a".into()],
                },
            }),
        });
        check_report(&resume.applied[0].dist);
        let failed = resume.failed.unwrap();
        assert_eq!(
            (failed.original.as_str(), failed.fork.as_str()),
            ("vendor/y", "me/y")
        );
        assert_eq!(
            (failed.error_kind.as_str(), failed.error_message.as_str()),
            ("incompatible_replacement", "changed")
        );

        let plan = UninstallPlan::from(pm::UninstallPlan {
            id: "me/x".into(),
            registered: false,
            deletes: vec!["/w/packages/me/x".into(), "/w/dist/me/x".into()],
            lost_work: all_lost_work(),
            dependents: vec![],
            dependency_conflict: Some("cannot uninstall me/x, which would reactivate vendor/x: lib is disabled; enable it".into()),
        });
        assert!(!plan.registered);
        assert!(
            plan.dependency_conflict.is_some(),
            "dependencyConflict is mirrored"
        );
        assert_eq!(plan.deletes, ["/w/packages/me/x", "/w/dist/me/x"]);
        assert_eq!(plan.lost_work.len(), 8);
        assert!(matches!(
            plan.lost_work[5],
            LostWork::UnpublishedBranch { commits: 7, .. }
        ));
    }

    /// Every CLI command that reads or changes a workspace, reads a catalog or checks
    /// access has an FFI call returning what it prints; the author commands and the
    /// catalog writers are the stated exceptions.
    #[test]
    fn ffi_covers_every_workspace_and_catalog_read_command() {
        // The references fail to compile when a call is removed or renamed.
        let covered: Vec<(&str, Vec<&str>)> = vec![
            ("auth", {
                let _ = auth_check;
                vec!["authCheck"]
            }),
            ("catalog show", {
                let _ = browse_catalog;
                let _ = PackageManager::browse_catalog;
                vec!["browseCatalog"]
            }),
            ("catalog links", {
                let _ = catalog_links;
                vec!["catalogLinks"]
            }),
            ("install-catalog", {
                let _ = PackageManager::install_from_catalog;
                let _ = catalog_release;
                vec!["installFromCatalog", "catalogRelease"]
            }),
            ("install", {
                let _ = PackageManager::install;
                vec!["install"]
            }),
            ("list", {
                let _ = list;
                let _ = read_state;
                vec!["list", "readState"]
            }),
            ("check", {
                let _ = check;
                vec!["check"]
            }),
            ("update", {
                let _ = PackageManager::update;
                let _ = PackageManager::update_all;
                let _ = PackageManager::update_all_plan;
                vec!["update", "updateAll", "updateAllPlan"]
            }),
            ("rollback", {
                let _ = PackageManager::rollback;
                vec!["rollback"]
            }),
            ("enable", {
                let _ = PackageManager::enable;
                vec!["enable"]
            }),
            ("disable", {
                let _ = PackageManager::disable;
                vec!["disable"]
            }),
            ("download", {
                let _ = PackageManager::download;
                vec!["download"]
            }),
            ("download-catalog", {
                let _ = PackageManager::download_from_catalog;
                vec!["downloadFromCatalog"]
            }),
            ("forget", {
                let _ = PackageManager::forget;
                vec!["forget"]
            }),
            ("restore", {
                let _ = PackageManager::restore;
                vec!["restore"]
            }),
            ("uninstall", {
                let _ = PackageManager::uninstall_plan;
                let _ = PackageManager::uninstall;
                let _ = describe_lost_work;
                let _ = uninstall_refusal;
                vec!["uninstallPlan", "uninstall", "uninstallRefusal"]
            }),
            ("fork", {
                let _ = PackageManager::fork_packages;
                let _ = PackageManager::fork_packages_and_replace;
                let _ = PackageManager::fork_replace_resume;
                vec![
                    "forkPackages",
                    "forkPackagesAndReplace",
                    "forkReplaceResume",
                ]
            }),
            ("replace", {
                let _ = PackageManager::replace_plan;
                let _ = PackageManager::replace;
                let _ = PackageManager::switch_back_plan;
                let _ = PackageManager::switch_back;
                vec!["replacePlan", "replace", "switchBackPlan", "switchBack"]
            }),
            ("sync-dist", {
                let _ = PackageManager::sync_dist_plan;
                let _ = PackageManager::sync_dist;
                vec!["syncDistPlan", "syncDist"]
            }),
        ];
        let exceptions = [
            "init",
            "commit",
            "remote",
            "catalog",
            "catalog update",
            "catalog register",
        ];
        let cli = include_str!("../../gitpm-cli/src/main.rs");
        let block = cli
            .split("const COMMANDS: &[&str] = &[")
            .nth(1)
            .and_then(|rest| rest.split("];").next())
            .expect("the CLI lists its commands");
        let commands: Vec<&str> = block.split('"').skip(1).step_by(2).collect();
        assert!(commands.len() > 20, "{commands:?}");
        for command in &commands {
            assert!(
                covered.iter().any(|(c, _)| c == command) || exceptions.contains(command),
                "CLI command {command} has no FFI equivalent"
            );
        }
        for (command, _) in &covered {
            assert!(commands.contains(command), "{command} is no CLI command");
        }
    }

    #[test]
    fn check_rows_carry_status_and_message() {
        use pm::check::CheckStatus as S;
        let statuses = [
            (S::DifferentSource, "DifferentSource"),
            (S::InvalidCatalogData, "InvalidCatalogData"),
            (S::NoVersions, "NoVersions"),
            (S::UncomparableVersion, "UncomparableVersion"),
            (S::SameVersionDifferentCommit, "SameVersionDifferentCommit"),
            (S::UpdateAvailable, "UpdateAvailable"),
            (S::UpToDate, "UpToDate"),
            (S::NewerThanCatalog, "NewerThanCatalog"),
            (S::OtherCatalog, "OtherCatalog"),
            (S::NewerThanOrigin, "NewerThanOrigin"),
            (S::NoReleases, "NoReleases"),
            (S::OriginUnreachable, "OriginUnreachable"),
            (S::Pinned, "Pinned"),
            (S::UpstreamNotInCatalog, "UpstreamNotInCatalog"),
        ];
        for (status, name) in statuses {
            let row = CheckRow::from(pm::check::CheckRow {
                id: "me/x".into(),
                installed: "1.0.0".into(),
                latest: Some("1.1.0".into()),
                enabled: false,
                update_available: status == S::UpdateAvailable,
                status,
                source: pm::check::CheckSource::Catalog,
                mode: pm::Mode::Release,
                problem: None,
                origin_latest: (status == S::UpstreamNotInCatalog).then(|| "1.2.0".into()),
                upstream: None,
                major: false,
                prerelease: None,
            });
            assert_eq!(format!("{:?}", row.status), name);
            if status == S::UpstreamNotInCatalog {
                assert_eq!(row.status_message, "upstream 1.2.0 not in catalog");
                assert_eq!(row.origin_latest.as_deref(), Some("1.2.0"));
            } else {
                assert_eq!(row.status_message, status.message());
            }
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
            Build, CatalogOverview as O, FileDigest, OverviewItem as I, PackageSummary as P,
            Release,
        };
        let overview = CatalogOverview::from(O {
            location: "https://pkg.example".into(),
            remote: true,
            verified: true,
            items: vec![
                I::Directory {
                    depth: 1,
                    name: "namespaces".into(),
                },
                I::Empty { depth: 2 },
                I::Package {
                    depth: 3,
                    package: P {
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
                overview.verified,
                overview.truncated
            ),
            ("https://pkg.example", true, true, true)
        );
        assert!(
            matches!(&overview.items[0], OverviewItem::Directory { depth: 1, name } if name == "namespaces")
        );
        assert!(matches!(
            overview.items[1],
            OverviewItem::Empty { depth: 2 }
        ));
        let OverviewItem::Package { depth: 3, package } = &overview.items[2] else {
            panic!()
        };
        assert_eq!(package.versions, ["1.9.0", "1.10.0"]);
        assert_eq!(package.latest.as_deref(), Some("1.10.0"));
        assert_eq!(
            (package.title.as_deref(), package.description.as_deref()),
            (Some("X"), Some("An x"))
        );
        assert_eq!(package.provides, ["theme"]);
        assert!(package.tags.is_empty());

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
            catalog_release("/nonexistent-pm-catalog".into(), "x".into(), None),
            Err(PmError::InvalidId { .. })
        ));
        assert!(matches!(
            catalog_release(
                "/nonexistent-pm-catalog".into(),
                "acme/x".into(),
                Some("1.0".into())
            ),
            Err(PmError::InvalidVersion { .. })
        ));
        // A local catalog location that does not exist is refused, not "no catalog".
        assert!(matches!(
            catalog_release("/nonexistent-pm-catalog".into(), "acme/x".into(), None),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            catalog_links("/nonexistent-pm-catalog".into()),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            PackageManager::open(
                std::env::temp_dir()
                    .join(format!("pm-ffi-never-{}", std::process::id()))
                    .to_string_lossy()
                    .into(),
                ManagerOptions {
                    catalog: Some("/nonexistent-pm-catalog".into()),
                    custom_ssh: false,
                }
            ),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(
            !std::env::temp_dir()
                .join(format!("pm-ffi-never-{}", std::process::id()))
                .exists()
        );
        // A workspace that does not exist yet reads as empty and is not created.
        let missing = std::env::temp_dir().join(format!("pm-ffi-missing-{}", std::process::id()));
        let view = read_state(missing.to_string_lossy().into()).unwrap();
        assert!(view.packages.is_empty() && view.resolved.is_empty());
        assert!(list(missing.to_string_lossy().into(), Some("theme".into())).is_ok());
        assert!(
            check(missing.to_string_lossy().into(), None, false)
                .unwrap()
                .rows
                .is_empty()
        );
        assert!(!missing.exists());
    }

    #[test]
    fn download_from_catalog_then_enable() {
        let dir = std::env::temp_dir().join(format!("pm-ffi-download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = pm::scaffold::init(&dir.join("hello"), "dan", "hello")
            .unwrap()
            .repo;
        let catalog = dir.join("catalog").to_string_lossy().into_owned();
        pm::catalog::register(&catalog, repo.to_str().unwrap(), "dan/hello@0.1.0").unwrap();
        let manager = PackageManager::open(
            dir.join("workspace").to_string_lossy().into(),
            ManagerOptions {
                catalog: Some(catalog),
                custom_ssh: true,
            },
        )
        .unwrap();
        let downloaded = manager
            .download_from_catalog("dan/hello".into(), None)
            .unwrap();
        assert!(!downloaded.enabled && !downloaded.already_installed);
        assert_eq!(downloaded.current.version, "0.1.0");
        assert!(downloaded.dist.deployed.is_empty());
        assert!(!dir.join("workspace/dist/dan/hello").exists());
        let listed = manager.list().unwrap();
        assert!(!listed[0].enabled);
        assert_eq!(
            manager.enable("dan/hello".into(), false).unwrap().deployed,
            ["dan/hello"]
        );
        assert!(dir.join("workspace/dist/dan/hello").is_dir());
        drop(manager);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_pinned_package_stays_with_its_reason() {
        let dir = std::env::temp_dir().join(format!("pm-ffi-pinned-{}", std::process::id()));
        let root = dir.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("state.json"),
            r#"{"format":1,"packages":{"acme/greeter":{"source":"s","current":{"version":"0.1.0","commit":"c"},"previous":null,"enabled":false,"mode":{"pinned":"acme/greeter@0.1.0"},"metadata":{}}}}"#,
        )
        .unwrap();
        let manager =
            PackageManager::open(root.to_string_lossy().into(), ManagerOptions::default()).unwrap();
        let pinned = Mode::Pinned {
            reference: "acme/greeter@0.1.0".into(),
        };
        assert_eq!(manager.list().unwrap()[0].mode, pinned);
        let view = read_state(root.to_string_lossy().into()).unwrap();
        assert_eq!(view.packages["acme/greeter"].mode, pinned);
        let change = manager
            .update("acme/greeter".into(), Target::Recorded, false)
            .unwrap();
        assert!(!change.changed);
        assert_eq!(
            change.unchanged_reason,
            Some(UnchangedReason::Pinned {
                reference: "acme/greeter@0.1.0".into()
            })
        );
        let report = check(root.to_string_lossy().into(), None, false).unwrap();
        assert!(matches!(report.rows[0].status, CheckStatus::Pinned));
        drop(manager);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_state_and_manager_report_state_errors() {
        let dir = std::env::temp_dir().join(format!("pm-ffi-state-{}", std::process::id()));
        let root = dir.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        // Another state format is refused, never rewritten.
        let old = r#"{"format":2,"plugins":{}}"#;
        std::fs::write(root.join("state.json"), old).unwrap();
        assert!(matches!(
            read_state(root.to_string_lossy().into()),
            Err(PmError::Unsupported { .. })
        ));
        assert!(matches!(
            PackageManager::open(root.to_string_lossy().into(), ManagerOptions::default()),
            Err(PmError::Unsupported { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(root.join("state.json")).unwrap(),
            old
        );
        std::fs::write(
            root.join("state.json"),
            r#"{"format":1,"packages":{"me/x":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":false,"forkOf":{"id":"vendor/x","version":"0.9.0","commit":"f"},"metadata":{}}}}"#,
        )
        .unwrap();
        let view = read_state(root.to_string_lossy().into()).unwrap();
        let list = view.resolved;
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), list[0].enabled), ("me/x", false));
        assert_eq!(list[0].fork_of.as_ref().unwrap().id, "vendor/x");
        assert_eq!(view.format, 1);
        assert_eq!(view.packages["me/x"].current.version, "1.0.0");

        let manager =
            PackageManager::open(root.to_string_lossy().into(), ManagerOptions::default()).unwrap();
        assert_eq!(manager.root(), root.to_string_lossy());
        assert_eq!(manager.list().unwrap()[0].current.version, "1.0.0");
        // Without a catalog check asks the origin; "s" is unreachable: a row, no error.
        let report = check(root.to_string_lossy().into(), None, true).unwrap();
        assert!(matches!(
            report.rows[0].status,
            CheckStatus::OriginUnreachable
        ));
        assert_eq!(report.rows[0].source, CheckSource::Origin);
        assert!(matches!(
            manager.browse_catalog(),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.enable("../x".into(), false),
            Err(PmError::InvalidId { .. })
        ));
        assert!(
            matches!(manager.rollback("me/missing".into()), Err(PmError::NotInstalled { id, .. }) if id == "me/missing")
        );
        assert!(matches!(
            manager.install("-x".into(), InstallTarget::Dev, None, true),
            Err(PmError::InvalidArgument { .. })
        ));
        assert!(matches!(
            manager.update(
                "me/missing".into(),
                Target::Ref {
                    reference: "main".into()
                },
                false
            ),
            Err(PmError::NotInstalled { .. })
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
            read_state(root.to_string_lossy().into()),
            Err(PmError::InvalidData { .. })
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
