//! Tilr plugin manager core.
//!
//! Plugins are Git repositories with a `plugin.json` manifest and a ready `dist/` folder.
//! A [`Manager`] owns one locked workspace (`plugins/` clones, deployed `dist/`,
//! `state.json`) and applies every change transactionally. Catalog functions read and
//! write a static JSON catalog (local folder or HTTP). Author functions in [`scaffold`]
//! create plugin repositories and release tags.
//!
//! The library is not interactive: it never prints, prompts or reads environment
//! variables. Settings are passed explicitly, results are returned as data, failures
//! as [`Error`] with a stable [`Error::kind`]. All calls block; run them off the UI thread.
//! Subprocesses (system `git`, `curl`) are confined to one internal module.

pub mod auth;
pub mod catalog;
pub mod check;
mod error;
mod exec;
pub mod identity;
mod lock;
mod lost_work;
mod model;
pub mod releases;
pub mod scaffold;
#[cfg(test)]
mod test_support;
mod workspace;

pub use error::{Error, Result};
pub use lost_work::LostWork;
pub use model::{ForkOf, Installed, InstalledPlugin, Manifest, Revision, State, valid_id};
pub use workspace::{InstallResult, Manager, Options, RevisionChange, UninstallPlan};
