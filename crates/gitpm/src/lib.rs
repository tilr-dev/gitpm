//! gitpm: an embeddable package manager where packages are folders in Git repositories:
//! releases as tags, a catalog, dependency resolution, forks and transactional installs.
//!
//! A package is a folder with a `plugin.json` manifest and a ready `dist/` folder: the root
//! of a Git repository, or one of the folders its `gitpm.json` lists (see [`packages`]).
//! A [`Manager`] owns one locked workspace (clones under `repos/`,
//! deployed `dist/`, `state.json`) and applies every change transactionally. Catalog
//! functions read and write a static JSON catalog (local folder or HTTP). Author
//! functions in [`scaffold`] create package repositories and release tags.
//!
//! The library is not interactive: it never prints, prompts or reads environment
//! variables. Settings are passed explicitly, results are returned as data, failures
//! as [`Error`] with a stable [`Error::kind`]. All calls block; run them off the UI thread.
//! Subprocesses (system `git`, `curl`) are confined to one internal module.

mod atomic_write;
pub mod auth;
pub mod catalog;
pub mod check;
pub mod dependency_edit;
pub mod deps;
mod error;
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
mod transport;
mod txn;
mod upgrade;
mod workspace;

pub use dependency_edit::{DependencyEdit, PackageDetails, edit_dependency, repository_packages};
pub use error::{Error, Result};
pub use list::{ListOptions, ListView, list_with};
pub use lost_work::LostWork;
pub use model::{
    ForkOf, Installed, InstalledPackage, LinkDescriptor, LinkDiscovery, Manifest, Metadata, Mode,
    Replaces, Revision, STATE_FORMAT, State, valid_id,
};
pub use workspace::fork;
pub use workspace::{
    DistSync, ForgetResult, ForkedPackage, HeldBack, HeldBackReason, IncompatibleConsumer,
    InstallResult, InstallTarget, InstalledDependency, LinkResult, Manager, Options, PackageFork,
    ReplacementCheck, ReplacementIssue, ReplacementPlan, ReplacementResolution, RevisionChange,
    SkippedDist, Target, Unchanged, UnchangedReason, UninstallPlan, UpdateAllPlan, UpdateOptions,
};

mod artifact;
