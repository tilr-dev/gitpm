//! Workspace transactions that stage only what an operation touches.
//!
//! An operation prepares new package clones and `dist/` folders and the next
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

/// Paths (relative to the root, e.g. `packages/dan/demo1`) and what commit does with them.
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
/// Every directory between the root and a managed path (`packages`, `packages/<ns>`,
/// `dist/<ns>`) must be a real directory: through a symlink a write or delete would
/// land outside the workspace.
fn ensure_managed_parents(root: &Path, relative: &str) -> Result<()> {
    let mut current = root.to_path_buf();
    let parts: Vec<&str> = relative.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::Workspace(format!(
                    "managed path is a symlink: {}",
                    current.display()
                )));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(Error::Workspace(format!(
                    "managed path is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.into()),
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
    /// Writable copy of an installed clone at `relative` (`packages/<id>` or
    /// `repos/<key>`; `live` is its exact path in the workspace). Symlinks inside the
    /// repository are kept as symlinks. Staged once per transaction.
    // TODO: this copies the whole clone (all packages of a shared one, full history) for
    // every operation that changes it; a worktree or a copy of `.git` plus the touched
    // folders would be cheaper for large repositories.
    pub(crate) fn repo(&mut self, relative: &str, live: &Path) -> Result<PathBuf> {
        ensure_managed_parents(&self.root, relative)?;
        let staged = self.staged(relative);
        if !self.is_staged(relative) {
            copy_tree(live, &staged, true)?;
            self.journal.paths.insert(relative.into(), Change::Replace);
        }
        Ok(staged)
    }
    /// The staged copy of the clone at `relative`, if this transaction staged one.
    pub(crate) fn staged_repo(&self, relative: &str) -> Option<PathBuf> {
        self.is_staged(relative).then(|| self.staged(relative))
    }
    /// Adopt a fresh clone (moved from the scratch area) as the clone at `relative`.
    pub(crate) fn add_repo(&mut self, relative: &str, clone: &Path) -> Result<PathBuf> {
        ensure_managed_parents(&self.root, relative)?;
        let staged = self.staged(relative);
        remove(&staged)?;
        fs::create_dir_all(staged.parent().unwrap())?;
        fs::rename(clone, &staged)?;
        self.journal.paths.insert(relative.into(), Change::Replace);
        Ok(staged)
    }
    /// Stage `dist/<id>` from `<package>/dist` at `commit` of the (staged or live) clone
    /// at `repo_relative`; `package` is the folder inside the repository (`None`: root).
    /// Read from Git objects (see [`crate::workspace::export_dist`]), so the working tree
    /// and HEAD of the clone do not matter.
    pub(crate) fn deploy(
        &mut self,
        id: &str,
        repo_relative: &str,
        package: Option<&str>,
        commit: &str,
    ) -> Result<()> {
        ensure_managed_parents(&self.root, repo_relative)?;
        let repo = match self.staged_repo(repo_relative) {
            Some(staged) => staged,
            None => self.root.join(repo_relative),
        };
        let staged = self.stage_dist(id)?;
        crate::workspace::export_dist(&repo, commit, package, &staged)
    }
    /// Remove a clone (`packages/<id>` or `repos/<key>`) on commit.
    pub(crate) fn delete_repo(&mut self, relative: &str) -> Result<()> {
        ensure_managed_parents(&self.root, relative)?;
        remove(&self.staged(relative))?;
        self.journal.paths.insert(relative.into(), Change::Delete);
        Ok(())
    }
    /// Remove the package's deployed files on commit (also drops a staged deploy).
    pub(crate) fn delete_dist(&mut self, id: &str) -> Result<()> {
        self.delete_repo(&format!("dist/{id}"))
    }
    /// An empty staged `dist/<id>` to fill (replaces the live one on commit).
    pub(crate) fn stage_dist(&mut self, id: &str) -> Result<PathBuf> {
        let relative = format!("dist/{id}");
        ensure_managed_parents(&self.root, &relative)?;
        let staged = self.staged(&relative);
        remove(&staged)?;
        fs::create_dir_all(&staged)?;
        self.journal.paths.insert(relative, Change::Replace);
        Ok(staged)
    }
    /// Whether this transaction deploys or deletes `dist/<id>`.
    pub(crate) fn has_dist(&self, id: &str) -> bool {
        self.journal.paths.contains_key(&format!("dist/{id}"))
    }
    /// IDs whose `dist/` this transaction deploys or deletes.
    pub(crate) fn staged_dists(&self) -> Vec<String> {
        self.journal
            .paths
            .keys()
            .filter_map(|relative| relative.strip_prefix("dist/"))
            .map(str::to_owned)
            .collect()
    }
    /// Whether this transaction deploys (rather than deletes) `dist/<id>`.
    pub(crate) fn deploys_dist(&self, id: &str) -> bool {
        self.journal.paths.get(&format!("dist/{id}")) == Some(&Change::Replace)
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

/// After a delete, remove the parents it left empty (`dist/<ns>`, `packages/<ns>`,
/// `repos`); `packages` and `dist` themselves stay, as `Manager::open` creates them.
/// Best effort: a folder that is not empty (or cannot be removed) is kept.
fn remove_empty_parents(root: &Path, relative: &str) {
    let parts: Vec<&str> = relative.split('/').collect();
    let keep = if matches!(parts[0], "packages" | "dist") {
        1
    } else {
        0
    };
    for depth in (keep + 1..parts.len()).rev() {
        if fs::remove_dir(root.join(parts[..depth].join("/"))).is_err() {
            break;
        }
    }
}

/// Idempotent: every step checks whether it already happened.
fn apply(root: &Path, dir: &Path) -> Result<()> {
    let journal: Journal = serde_json::from_slice(&fs::read(dir.join(JOURNAL))?)
        .map_err(|e| Error::Workspace(format!("corrupt transaction journal: {e}")))?;
    for relative in journal.paths.keys() {
        ensure_managed_parents(root, relative)?;
    }
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
                remove_empty_parents(root, relative);
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
