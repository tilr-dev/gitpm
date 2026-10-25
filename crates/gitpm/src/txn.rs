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

/// Paths (relative to the root, e.g. `repos/demo-0123456789abcdef`) and what commit does with them.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Journal {
    paths: BTreeMap<String, Change>,
}

pub(crate) struct Txn {
    root: PathBuf,
    dir: PathBuf,
    journal: Journal,
    committed: bool,
    pub(crate) warnings: Vec<String>,
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
/// Every directory between the root and a managed path (`dist/<ns>`) must be a real directory: through a symlink a write or delete would
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
/// Sync only real staged files/directories; repository symlinks must not be followed.
fn sync_tree(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    }
    sync_path(path)
}
fn sync_path(path: &Path) -> Result<()> {
    #[cfg(test)]
    SYNC_FAILURE.with(|failure| {
        let should_fail = failure.borrow().as_ref().is_some_and(|(target, after)| {
            target == path
                && after.as_ref().is_none_or(|destination| {
                    DURABILITY_EVENTS.with(|events| {
                        events.borrow().as_ref().is_some_and(|events| {
                            events.iter().any(|event| {
                                matches!(event,
                        DurabilityEvent::Rename(_, to) if to == destination)
                            })
                        })
                    })
                })
        });
        if should_fail {
            Err(Error::Io("simulated sync failure".into()))
        } else {
            Ok(())
        }
    })?;
    fs::File::open(path)?.sync_all()?;
    #[cfg(test)]
    record(DurabilityEvent::Sync(path.into()));
    Ok(())
}
/// Persist directory creation, including the entry in the first existing parent.
/// Existing ancestors above that boundary are neither changed nor opened.
pub(crate) fn create_workspace_dirs(path: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match fs::metadata(current) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current);
                current = current.parent().ok_or(e)?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    fs::create_dir_all(path)?;
    if !missing.is_empty() {
        for directory in missing {
            sync_path(directory)?;
        }
        sync_path(current)?;
    }
    Ok(())
}
/// Persist new ancestors child-first. Missing directories may have been pruned.
fn sync_parents(root: &Path, path: &Path) -> Result<()> {
    for parent in path.ancestors().take_while(|p| p.starts_with(root)) {
        match fs::symlink_metadata(parent) {
            Ok(_) => sync_path(parent)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn rename(root: &Path, from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to.parent().unwrap())?;
    #[cfg(test)]
    crash_point()?;
    fs::rename(from, to)?;
    #[cfg(test)]
    record(DurabilityEvent::Rename(from.into(), to.into()));
    // Persist the destination before allowing the source entry to disappear.
    sync_parents(root, to.parent().unwrap())?;
    sync_parents(root, from.parent().unwrap())?;
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
            warnings: Vec::new(),
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
    /// Writable copy of an installed clone at `relative` (`repos/<key>`; `live` is its exact path in the workspace). Symlinks inside the
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
        custom_ssh: bool,
    ) -> Result<PathBuf> {
        ensure_managed_parents(&self.root, repo_relative)?;
        let repo = match self.staged_repo(repo_relative) {
            Some(staged) => staged,
            None => self.root.join(repo_relative),
        };
        let staged = self.stage_dist(id)?;
        self.warnings.extend(crate::workspace::export_dist(
            &repo, commit, package, &staged, custom_ssh,
        )?);
        Ok(staged)
    }
    /// Stage a leaf symlink. Journal and cleanup own the entry, never its target.
    pub(crate) fn deploy_link(&mut self, id: &str, target: &Path) -> Result<()> {
        let staged = self.stage_dist(id)?;
        fs::remove_dir(&staged)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, &staged)?;
        #[cfg(not(unix))]
        return Err(Error::Unsupported(
            "live links require Unix symlinks".into(),
        ));
        Ok(())
    }
    /// Remove a clone (`repos/<key>`) on commit.
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
        // Payload data, symlink entries and state must survive before the decision.
        sync_tree(&self.dir)?;
        let temporary = self.dir.join("journal.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.journal)?)?;
        sync_path(&temporary)?;
        // From here on the transaction is decided: recovery rolls it forward.
        fs::rename(&temporary, self.dir.join(JOURNAL))?;
        self.committed = true;
        #[cfg(test)]
        record(DurabilityEvent::Rename(temporary, self.dir.join(JOURNAL)));
        sync_parents(&self.root, &self.dir)?;
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

/// After a delete, remove the parents it left empty (`dist/<ns>`, `repos`); `dist` itself stays, as `Manager::open` creates them.
/// Best effort: a folder that is not empty (or cannot be removed) is kept.
fn remove_empty_parents(root: &Path, relative: &str) {
    let parts: Vec<&str> = relative.split('/').collect();
    let keep = if parts[0] == "dist" { 1 } else { 0 };
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
    // Recovery can follow a failed sync immediately after journal publication.
    sync_path(&dir.join(JOURNAL))?;
    sync_parents(root, dir)?;
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
                        rename(root, &live, &old)?;
                    }
                    rename(root, &staged, &live)?;
                }
            }
            Change::Delete => {
                if present(&live) {
                    rename(root, &live, &old)?;
                }
                remove_empty_parents(root, relative);
            }
        }
        // Recovery may resume after a rename succeeded but its sync failed.
        sync_parents(root, live.parent().unwrap())?;
        sync_parents(root, staged.parent().unwrap())?;
        sync_parents(root, old.parent().unwrap())?;
    }
    let state = dir.join("state.json");
    if present(&state) {
        rename(root, &state, &root.join("state.json"))?;
    }
    sync_path(&root.join("state.json"))?;
    sync_path(root)?;
    remove(dir)?;
    sync_path(root)
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

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum DurabilityEvent {
    Sync(PathBuf),
    Rename(PathBuf, PathBuf),
}
#[cfg(test)]
thread_local! {
    static SYNC_FAILURE: std::cell::RefCell<Option<(PathBuf, Option<PathBuf>)>> = const { std::cell::RefCell::new(None) };
    static DURABILITY_EVENTS: std::cell::RefCell<Option<Vec<DurabilityEvent>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn record(event: DurabilityEvent) {
    DURABILITY_EVENTS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push(event);
        }
    });
}

#[cfg(test)]
#[test]
fn commit_syncs_journal_state_and_parents() {
    let temp = crate::test_support::TempDir::new("txn-sync");
    let root = temp.path();
    let mut txn = Txn::begin(root).unwrap();
    // Stage directly to exercise nested new parents, replacements and deletion.
    for relative in ["dist/new/plugin", "repos/existing-0123456789abcdef"] {
        let path = txn.dir.join("new").join(relative);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("payload"), b"new").unwrap();
        txn.journal.paths.insert(relative.into(), Change::Replace);
    }
    for relative in ["repos/existing-0123456789abcdef", "dist/deleted/plugin"] {
        fs::create_dir_all(root.join(relative)).unwrap();
        fs::write(root.join(relative).join("payload"), b"old").unwrap();
    }
    txn.journal
        .paths
        .insert("dist/deleted/plugin".into(), Change::Delete);
    let _capture = Capture::new();
    txn.commit(&State::default()).unwrap();
    let events = DURABILITY_EVENTS.with(|events| events.borrow_mut().take().unwrap());
    let sync_index = |path: &Path| {
        events
            .iter()
            .position(|event| event == &DurabilityEvent::Sync(path.into()))
            .unwrap_or_else(|| panic!("missing sync for {}: {events:?}", path.display()))
    };
    let journal = sync_index(&root.join(DIR).join("journal.tmp"));
    let state = sync_index(&root.join(DIR).join("state.json"));
    assert!(state < journal);
    assert!(events.iter().all(|event| match event {
        DurabilityEvent::Sync(path) => path.starts_with(root),
        _ => true,
    }));
    for relative in ["dist/new/plugin", "repos/existing-0123456789abcdef"] {
        assert!(sync_index(&root.join(DIR).join("new").join(relative).join("payload")) < journal);
    }
    let published = events
        .iter()
        .position(|event| {
            matches!(event,
        DurabilityEvent::Rename(_, to) if to == &root.join(DIR).join(JOURNAL))
        })
        .unwrap();
    assert!(journal < published);
    let first_move = events
        .iter()
        .enumerate()
        .skip(published + 1)
        .find(|(_, event)| matches!(event, DurabilityEvent::Rename(..)))
        .unwrap()
        .0;
    for path in [root.join(DIR), root.to_path_buf()] {
        assert!(events[published + 1..first_move].contains(&DurabilityEvent::Sync(path)));
    }
    // Every rename must persist both parent chains before the next rename.
    for (index, event) in events.iter().enumerate().skip(first_move) {
        if let DurabilityEvent::Rename(from, to) = event {
            let end = events[index + 1..]
                .iter()
                .position(|e| matches!(e, DurabilityEvent::Rename(..)))
                .map_or(events.len(), |offset| index + 1 + offset);
            for path in [from.parent().unwrap(), to.parent().unwrap()] {
                for parent in path.ancestors().take_while(|p| p.starts_with(root)) {
                    assert!(
                        events[index + 1..end].contains(&DurabilityEvent::Sync(parent.into())),
                        "missing parent sync after {event:?}: {}",
                        parent.display()
                    );
                }
            }
        }
    }
    assert_eq!(events.last(), Some(&DurabilityEvent::Sync(root.into())));
    assert!(!root.join(DIR).exists());
    assert_eq!(
        fs::read(root.join("dist/new/plugin/payload")).unwrap(),
        b"new"
    );
    assert!(!root.join("dist/deleted").exists());
}

#[cfg(test)]
struct Capture;
#[cfg(test)]
impl Capture {
    fn new() -> Self {
        DURABILITY_EVENTS.with(|events| *events.borrow_mut() = Some(Vec::new()));
        Self
    }
}
#[cfg(test)]
impl Drop for Capture {
    fn drop(&mut self) {
        DURABILITY_EVENTS.with(|events| *events.borrow_mut() = None);
        SYNC_FAILURE.with(|failure| *failure.borrow_mut() = None);
    }
}

#[cfg(test)]
#[test]
fn sync_errors_preserve_the_commit_decision_and_recovery() {
    for phase in ["journal", "published", "moved", "state"] {
        let temp = crate::test_support::TempDir::new("txn-sync-failure");
        let root = temp.path();
        let mut txn = Txn::begin(root).unwrap();
        let relative = "dist/ns/plugin";
        let staged = txn.dir.join("new").join(relative);
        fs::create_dir_all(&staged).unwrap();
        fs::write(staged.join("payload"), b"new").unwrap();
        txn.journal.paths.insert(relative.into(), Change::Replace);
        fs::create_dir_all(root.join(relative)).unwrap();
        fs::write(root.join(relative).join("payload"), b"old").unwrap();
        let _capture = Capture::new();
        let (target, after) = match phase {
            "journal" => (root.join(DIR).join("journal.tmp"), None),
            "published" => (root.join(DIR), Some(root.join(DIR).join(JOURNAL))),
            "moved" => (root.join("dist/ns"), Some(root.join(relative))),
            "state" => (root.to_path_buf(), Some(root.join("state.json"))),
            _ => unreachable!(),
        };
        SYNC_FAILURE.with(|failure| *failure.borrow_mut() = Some((target, after)));
        let error = txn.commit(&State::default()).unwrap_err();
        assert!(
            error.to_string().contains("simulated sync failure"),
            "{error}"
        );
        SYNC_FAILURE.with(|failure| *failure.borrow_mut() = None);
        if phase == "journal" {
            assert!(!root.join(DIR).exists());
            assert_eq!(
                fs::read(root.join(relative).join("payload")).unwrap(),
                b"old"
            );
        } else {
            assert!(root.join(DIR).join(JOURNAL).exists());
            if phase == "published" {
                assert_eq!(
                    fs::read(root.join(relative).join("payload")).unwrap(),
                    b"old"
                );
            }
            recover(root).unwrap();
            assert_eq!(
                fs::read(root.join(relative).join("payload")).unwrap(),
                b"new"
            );
            assert!(root.join("state.json").exists());
            assert!(!root.join(DIR).exists());
        }
    }
}

#[cfg(all(test, unix))]
#[test]
fn commit_syncs_symlink_entries_without_following_targets() {
    use std::os::unix::fs::symlink;
    let temp = crate::test_support::TempDir::new("txn-sync-symlink");
    let root = temp.path();
    let mut txn = Txn::begin(root).unwrap();
    let staged = txn.dir.join("new/repos/plugin-0123456789abcdef");
    fs::create_dir_all(&staged).unwrap();
    symlink("missing-target", staged.join("link")).unwrap();
    txn.journal
        .paths
        .insert("repos/plugin-0123456789abcdef".into(), Change::Replace);
    let _capture = Capture::new();
    txn.commit(&State::default()).unwrap();
    assert_eq!(
        fs::read_link(root.join("repos/plugin-0123456789abcdef/link")).unwrap(),
        Path::new("missing-target")
    );
    DURABILITY_EVENTS.with(|events| {
        let events = events.borrow();
        let events = events.as_ref().unwrap();
        assert!(events.contains(&DurabilityEvent::Sync(staged.clone())));
        assert!(!events.contains(&DurabilityEvent::Sync(staged.join("link"))));
    });
}

#[cfg(test)]
#[test]
fn workspace_creation_syncs_only_new_ancestors() {
    let temp = crate::test_support::TempDir::new("txn-new-ancestors");
    let existing = temp.join("existing");
    fs::create_dir(&existing).unwrap();
    let parent = existing.join("new");
    let root = parent.join("workspace");
    let _capture = Capture::new();
    // Exercise the public creation path, including parents made before locking.
    let manager = crate::Manager::open(&root).unwrap();
    let events = DURABILITY_EVENTS.with(|events| events.borrow_mut().take().unwrap());
    let synced: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            DurabilityEvent::Sync(path) => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(synced, [&parent, &existing, &root, &parent]);
    DURABILITY_EVENTS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    drop(manager);
    let _manager = crate::Manager::open(&root).unwrap();
    assert!(DURABILITY_EVENTS.with(|events| events.borrow().as_ref().unwrap().is_empty()));
}
