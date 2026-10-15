//! Workspace transactions that stage only what an operation touches.
//!
//! An operation prepares new plugin clones and `dist/` folders and the next
//! `state.json` inside `<root>/.pm-txn`; the live workspace is untouched until commit.
//! Commit writes a journal and then moves each staged path into place (the replaced
//! one goes to `.pm-txn/old`), `state.json` last via an atomic rename. A crash before
//! the journal leaves the workspace as it was; after it, [`recover`] rolls forward.
//! Readers of `state.json` always see a complete file.
use crate::error::{Error, Result};
use crate::{State, workspace::copy_tree};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) const DIR: &str = ".pm-txn";
const JOURNAL: &str = "journal.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Change {
    Replace,
    Delete,
}

/// Paths (relative to the root, e.g. `plugins/dan/demo1`) and what commit does with them.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Journal {
    paths: BTreeMap<String, Change>,
}

pub(crate) struct Txn {
    root: PathBuf,
    dir: PathBuf,
    journal: Journal,
    committed: bool,
}

fn remove(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        if fs::symlink_metadata(path)?.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
fn rename(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to.parent().unwrap())?;
    #[cfg(test)]
    crash_point()?;
    fs::rename(from, to)?;
    Ok(())
}

impl Txn {
    /// Start a transaction; a leftover committed journal is rolled forward first.
    pub(crate) fn begin(root: &Path) -> Result<Self> {
        recover(root)?;
        let dir = root.join(DIR);
        fs::create_dir(&dir)?;
        Ok(Self {
            root: root.to_path_buf(),
            dir,
            journal: Journal::default(),
            committed: false,
        })
    }
    /// The live workspace root (read-only during the transaction).
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    /// Scratch directory inside the transaction for temporary clones.
    pub(crate) fn scratch(&self, name: &str) -> PathBuf {
        self.dir.join("scratch").join(name)
    }
    fn staged(&self, relative: &str) -> PathBuf {
        self.dir.join("new").join(relative)
    }
    fn is_staged(&self, relative: &str) -> bool {
        self.journal.paths.get(relative) == Some(&Change::Replace)
    }
    /// Writable copy of an installed clone (`live` is its exact path in the workspace).
    /// Symlinks inside the repository are kept as symlinks.
    pub(crate) fn repo(&mut self, id: &str, live: &Path) -> Result<PathBuf> {
        let relative = format!("plugins/{id}");
        let staged = self.staged(&relative);
        if !self.is_staged(&relative) {
            copy_tree(live, &staged, true)?;
            self.journal.paths.insert(relative, Change::Replace);
        }
        Ok(staged)
    }
    /// Adopt a fresh clone (moved from the scratch area) as the plugin's repository.
    pub(crate) fn add_repo(&mut self, id: &str, clone: &Path) -> Result<PathBuf> {
        let relative = format!("plugins/{id}");
        let staged = self.staged(&relative);
        fs::create_dir_all(staged.parent().unwrap())?;
        fs::rename(clone, &staged)?;
        self.journal.paths.insert(relative, Change::Replace);
        Ok(staged)
    }
    /// Stage `dist/<id>` from the plugin's (staged or live) `dist/`; symlinks refused.
    pub(crate) fn deploy(&mut self, id: &str) -> Result<()> {
        let repo_relative = format!("plugins/{id}");
        let repo = if self.is_staged(&repo_relative) {
            self.staged(&repo_relative)
        } else {
            self.root.join(&repo_relative)
        };
        let relative = format!("dist/{id}");
        let staged = self.staged(&relative);
        remove(&staged)?;
        copy_tree(&repo.join("dist"), &staged, false)?;
        self.journal.paths.insert(relative, Change::Replace);
        Ok(())
    }
    /// Remove the plugin's repository and deployed files on commit.
    pub(crate) fn delete(&mut self, id: &str) -> Result<()> {
        for relative in [format!("plugins/{id}"), format!("dist/{id}")] {
            remove(&self.staged(&relative))?;
            self.journal.paths.insert(relative, Change::Delete);
        }
        Ok(())
    }
    pub(crate) fn commit(mut self, state: &State) -> Result<()> {
        fs::write(
            self.dir.join("state.json"),
            serde_json::to_vec_pretty(state)?,
        )?;
        remove(&self.dir.join("scratch"))?;
        let temporary = self.dir.join("journal.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.journal)?)?;
        // From here on the transaction is decided: recovery rolls it forward.
        fs::rename(&temporary, self.dir.join(JOURNAL))?;
        self.committed = true;
        apply(&self.root, &self.dir)
    }
}
impl Drop for Txn {
    /// Not committed: discard everything staged; the live workspace was never touched.
    fn drop(&mut self) {
        if !self.committed {
            let _ = remove(&self.dir);
        }
    }
}

/// Finish an interrupted transaction: roll a committed journal forward, discard an
/// uncommitted stage.
pub(crate) fn recover(root: &Path) -> Result<()> {
    let dir = root.join(DIR);
    if !present(&dir) {
        return Ok(());
    }
    if present(&dir.join(JOURNAL)) {
        apply(root, &dir)
    } else {
        remove(&dir)
    }
}

/// Idempotent: every step checks whether it already happened.
fn apply(root: &Path, dir: &Path) -> Result<()> {
    let journal: Journal = serde_json::from_slice(&fs::read(dir.join(JOURNAL))?)
        .map_err(|e| Error::Workspace(format!("corrupt transaction journal: {e}")))?;
    for (relative, change) in &journal.paths {
        let live = root.join(relative);
        let staged = dir.join("new").join(relative);
        let old = dir.join("old").join(relative);
        match change {
            Change::Replace => {
                if present(&staged) {
                    if present(&live) {
                        rename(&live, &old)?;
                    }
                    rename(&staged, &live)?;
                }
            }
            Change::Delete => {
                if present(&live) {
                    rename(&live, &old)?;
                }
            }
        }
    }
    let state = dir.join("state.json");
    if present(&state) {
        rename(&state, &root.join("state.json"))?;
    }
    remove(dir)
}

#[cfg(test)]
thread_local! {
    /// Number of renames allowed before a simulated crash (`None`: no crash).
    pub(crate) static CRASH_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
fn crash_point() -> Result<()> {
    CRASH_AFTER.with(|left| match left.get() {
        Some(0) => Err(Error::Io("simulated crash".into())),
        Some(n) => {
            left.set(Some(n - 1));
            Ok(())
        }
        None => Ok(()),
    })
}
