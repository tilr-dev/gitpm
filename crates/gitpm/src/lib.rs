//! gitpm: an embeddable package manager where packages are folders in Git repositories:
//! releases as tags, a catalog, dependency resolution, forks and transactional installs.
//!
//! A package is a folder with a `plugin.json` manifest and a ready `dist/` folder: the root
//! of a Git repository, or one of the folders its `gitpm.json` lists (see [`packages`]).
//! A [`Manager`] owns one locked workspace (clones under `packages/` and `repos/`,
//! deployed `dist/`, `state.json`) and applies every change transactionally. Catalog
//! functions read and write a static JSON catalog (local folder or HTTP). Author
//! functions in [`scaffold`] create package repositories and release tags.
//!
//! The library is not interactive: it never prints, prompts or reads environment
//! variables. Settings are passed explicitly, results are returned as data, failures
//! as [`Error`] with a stable [`Error::kind`]. All calls block; run them off the UI thread.
//! Subprocesses (system `git`, `curl`) are confined to one internal module.

pub mod auth;
pub mod catalog;
pub mod check;
pub mod deps;
mod error;
mod exec;
pub mod identity;
pub mod list;
mod lock;
mod lost_work;
mod model;
pub mod packages;
pub mod releases;
pub mod scaffold;
#[cfg(test)]
mod test_support;
mod txn;
mod upgrade;
mod workspace;

pub use error::{Error, Result};
pub use list::{ListOptions, ListView, list_with};
pub use lost_work::LostWork;
pub use model::{
    ForkOf, Installed, InstalledPackage, Manifest, Metadata, Mode, Replaces, Revision,
    STATE_FORMAT, State, valid_id,
};
pub use workspace::{
    DependencyRewrite, DistSync, ForgetResult, ForkReplacement, ForkResume, ForkStep,
    ForkedPackage, HeldBack, HeldBackReason, IncompatibleConsumer, InstallResult, InstallTarget,
    InstalledDependency, Manager, Options, PackageFork, PackageForkReplacement, PendingReplacement,
    ReplacementPlan, ResumeFailure, RevisionChange, SkippedDist, Target, Unchanged,
    UnchangedReason, UninstallPlan, UpdateAllPlan, UpdateOptions,
};
