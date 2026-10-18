//! Clones in the workspace: the own clone `packages/<id>` of a package, or the clone
//! `repos/<key>` shared by every package installed from one multi-package source. A
//! shared clone is used by its registered packages and by forgotten ones
//! (`State::forgotten`); it is removed with the last of them.
//!
//! A shared clone has one HEAD, but several installed revisions. Each user's current
//! and previous revision is kept reachable by `refs/gitpm/<id>/current` and
//! `refs/gitpm/<id>/previous`, so a fetch that prunes or overwrites the tag or branch
//! that held a sibling's revision (an upstream rewrite) can never let `git gc` drop it.
use super::*;

/// Prefix of the refs that keep the installed revisions of a shared clone reachable.
const KEEP: &str = "refs/gitpm/";

/// Make the `refs/gitpm/` refs of the shared clone `repo` match the users of `key` in
/// `state`: one ref per recorded current/previous revision, none for anything else (an
/// uninstalled package's refs go). A revision already missing from the clone is skipped.
pub(super) fn sync_refs(repo: &Path, state: &State, key: &str) -> Result<()> {
    let mut wanted = BTreeMap::new();
    for (id, p) in clone_users(state, key, "") {
        for (slot, revision) in [
            ("current", Some(&p.current)),
            ("previous", p.previous.as_ref()),
        ] {
            if let Some(revision) = revision.filter(|r| !r.commit.is_empty()) {
                wanted.insert(format!("{KEEP}{id}/{slot}"), revision.commit.clone());
            }
        }
    }
    let mut have = BTreeMap::new();
    for line in git(
        repo,
        &["for-each-ref", "--format=%(refname) %(objectname)", KEEP],
    )?
    .lines()
    {
        if let Some((name, object)) = line.split_once(' ') {
            have.insert(name.to_owned(), object.to_owned());
        }
    }
    for name in have.keys().filter(|name| !wanted.contains_key(*name)) {
        git(repo, &["update-ref", "-d", name])?;
    }
    for (name, commit) in &wanted {
        if have.get(name) == Some(commit)
            || git(repo, &["cat-file", "-e", &format!("{commit}^{{commit}}")]).is_err()
        {
            continue;
        }
        git(repo, &["update-ref", name, commit])?;
    }
    Ok(())
}

/// Remove the workspace's own refs, `refs/gitpm/` and the recorded origin tags
/// (`refs/origin-tags/`), from a copy of a clone that is not one of its clones.
pub(super) fn drop_refs(repo: &Path) -> Result<()> {
    for name in git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)",
            KEEP,
            lost_work::ORIGIN_TAGS,
        ],
    )?
    .lines()
    {
        git(repo, &["update-ref", "-d", name])?;
    }
    Ok(())
}

/// Fetch the origin's tags into a shared clone as they are there (a moved tag is taken
/// at its new commit; with `prune`, deleted ones go), after protecting every installed
/// revision with [`sync_refs`]. Tags are not policed: installed revisions are kept by
/// commit.
pub(super) fn fetch_tags(repo: &Path, state: &State, key: &str, prune: bool) -> Result<()> {
    sync_refs(repo, state, key)?;
    fetch_origin(repo, prune)
}

/// `git fetch origin --force --tags` (with `prune`, also `--prune --prune-tags`, so the
/// clone mirrors the origin's tags), then record those tags as the origin's tags of the
/// last fetch ([`lost_work::record_origin_tags`]).
pub(super) fn fetch_origin(repo: &Path, prune: bool) -> Result<()> {
    let mut args = vec!["fetch", "origin"];
    if prune {
        args.extend(["--prune", "--prune-tags"]);
    }
    args.extend(["--force", "--tags"]);
    git(repo, &args)?;
    lost_work::record_origin_tags(repo)
}

/// The own clone `packages/<id>` (exact spelling, must be a Git repository).
pub(super) fn package_repo(root: &Path, id: &str) -> Result<PathBuf> {
    let plugins = root.join("packages");
    let repo = plugins.join(id);
    if !exact_path(&plugins, id)? || !repo.join(".git").is_dir() {
        return Err(Error::RepositoryNotFound { id: id.into() });
    }
    Ok(repo)
}

/// Cache key of the shared clone of a multi-package source, its folder under `repos/`:
/// a readable part of the source plus a hash of the exact source string. The only
/// place a source is turned into a folder name (see `model::valid_clone_key`).
pub(crate) fn repo_key(source: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = catalog::hex(&Sha256::digest(source.as_bytes()));
    let last = source
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("");
    let name: String = last
        .trim_end_matches(".git")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();
    let name = name.trim_start_matches('-');
    if name.is_empty() {
        hash[..16].to_owned()
    } else {
        format!("{name}-{}", &hash[..16])
    }
}

/// The live clone of a package (registered or forgotten).
pub(super) fn installed_repo(root: &Path, id: &str, p: &Installed) -> Result<PathBuf> {
    let Some(key) = &p.clone else {
        return package_repo(root, id);
    };
    let repo = root.join("repos").join(key);
    if !exact_path(&root.join("repos"), key)? || !repo.join(".git").is_dir() {
        return Err(Error::RepositoryNotFound { id: id.into() });
    }
    Ok(repo)
}

/// Users of the shared clone `key` other than `except`: registered packages, then
/// forgotten ones (each in ID order).
pub(super) fn clone_users<'a>(
    state: &'a State,
    key: &str,
    except: &str,
) -> Vec<(&'a str, &'a Installed)> {
    state
        .packages
        .iter()
        .chain(&state.forgotten)
        .filter(|(id, p)| *id != except && p.clone.as_deref() == Some(key))
        .map(|(id, p)| (id.as_str(), p))
        .collect()
}

/// A shared clone has one HEAD for several packages, so HEAD must be the installed
/// commit of one of its users (forgotten ones included) or a tagged commit; anything
/// else is local work.
pub(super) fn shared_head_moved(
    repo: &Path,
    state: &State,
    key: &str,
    id: &str,
) -> Result<Option<LostWork>> {
    let head = git(repo, &["rev-parse", "HEAD"])?;
    if clone_users(state, key, "")
        .iter()
        .any(|(_, p)| p.current.commit == head)
        || !git(repo, &["tag", "--points-at", "HEAD"])?.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(LostWork::HeadMoved {
        head,
        recorded: state
            .packages
            .get(id)
            .or_else(|| state.forgotten.get(id))
            .map(|p| p.current.commit.clone())
            .unwrap_or_default(),
    }))
}

/// Local-work guard before an operation copies and changes a clone: no file changes,
/// HEAD at the installed revision (any user's revision for a shared clone), no
/// unpublished branches/tags/stash.
pub(super) fn guard_clone(
    live: &Path,
    state: &State,
    id: &str,
    key: Option<&str>,
    head_message: &str,
    hidden_message: &str,
    for_update: bool,
) -> Result<()> {
    clean(live, id)?;
    match key {
        None => {
            let recorded = &state.packages[id].current.commit;
            lost_work::require_head(live, id, recorded, head_message)?;
        }
        Some(key) => {
            if let Some(item) = shared_head_moved(live, state, key, id)? {
                return Err(Error::LocalChanges {
                    id: id.into(),
                    message: head_message.into(),
                    items: vec![item],
                });
            }
        }
    }
    if for_update {
        lost_work::require_no_hidden_for_update(live, id, hidden_message)
    } else {
        lost_work::require_no_hidden(live, id, hidden_message)
    }
}

/// The URL `origin` is configured with, as written (`git remote get-url` would apply
/// `url.<base>.insteadOf` rewrites and no longer match the recorded source).
pub(super) fn configured_origin(repo: &Path) -> Option<String> {
    git(repo, &["config", "--get", "remote.origin.url"]).ok()
}

/// Stage the verified clone where the package lives: its own `packages/<id>`, or for a
/// package of a multi-package repository the clone `repos/<key>` shared by every package
/// of that source. An existing shared clone is reused (guarded like an update, fetched,
/// checked out at the verified commit); the fresh clone is then discarded.
/// Returns the staged clone, its workspace-relative path and the shared key.
pub(super) fn place_clone(
    txn: &mut Txn,
    state: &State,
    cloned: &Cloned,
    source: &str,
) -> Result<(PathBuf, String, Option<String>)> {
    let id = &cloned.manifest.id;
    if cloned.path.is_none() {
        let relative = format!("packages/{id}");
        let staged = txn.add_repo(&relative, &cloned.repo)?;
        return Ok((staged, relative, None));
    }
    let key = repo_key(source);
    let relative = format!("repos/{key}");
    let commit = git(&cloned.repo, &["rev-parse", "HEAD"])?;
    let live = txn.root().join(&relative);
    let staged = if let Some(staged) = txn.staged_repo(&relative) {
        staged
    } else if fs::symlink_metadata(&live).is_ok() {
        ensure!(
            live.join(".git").is_dir() && configured_origin(&live).as_deref() == Some(source),
            Workspace,
            "{relative} exists but is not the clone of {source}"
        );
        clean(&live, id)?;
        if let Some(item) = shared_head_moved(&live, state, &key, id)? {
            return Err(Error::LocalChanges {
                id: id.clone(),
                message: format!("the shared clone {relative} has local commits"),
                items: vec![item],
            });
        }
        lost_work::require_no_hidden_for_update(
            &live,
            id,
            &format!("the shared clone {relative} holds unpublished work"),
        )?;
        txn.repo(&relative, &live)?
    } else {
        let staged = txn.add_repo(&relative, &cloned.repo)?;
        return Ok((staged, relative, Some(key)));
    };
    fetch_tags(&staged, state, &key, false)?;
    git(&staged, &["checkout", "--detach", &commit])?;
    let m = package_manifest(&staged, cloned.path.as_deref())?;
    ensure!(
        serde_json::to_value(&m)? == serde_json::to_value(&cloned.manifest)?,
        VersionConflict,
        "package {id} differs between the shared clone and its source"
    );
    Ok((staged, relative, Some(key)))
}

/// After `id` stopped using the shared clone `key` (`rest` is the state without it as
/// a registered package), make HEAD the installed commit of a remaining user: registered
/// ones first, else forgotten ones. A HEAD left at the removed package's revision (an
/// untagged dev commit, say) would block every other package as moved HEAD. The
/// checkout keeps working-tree changes; when they conflict, the operation is refused.
pub(super) fn settle_head(
    txn: &mut Txn,
    rest: &State,
    key: &str,
    except: &str,
    relative: &str,
) -> Result<()> {
    let users = clone_users(rest, key, except);
    let registered: Vec<_> = users
        .iter()
        .filter(|(id, _)| rest.packages.contains_key(*id))
        .collect();
    let candidates = if registered.is_empty() {
        users.iter().collect()
    } else {
        registered
    };
    let Some((target_id, target)) = candidates.first() else {
        return Ok(());
    };
    let live = txn.root().join(relative);
    if !live.join(".git").is_dir() {
        return Ok(());
    }
    let repo = txn.staged_repo(relative).unwrap_or_else(|| live.clone());
    let head = git(&repo, &["rev-parse", "HEAD"])?;
    if candidates.iter().any(|(_, p)| p.current.commit == head) {
        return Ok(());
    }
    let staged = txn.repo(relative, &live)?;
    if let Err(error) = git(
        &staged,
        &["checkout", "-q", "--detach", &target.current.commit],
    ) {
        let items = match clean(&staged, target_id) {
            Err(Error::LocalChanges { items, .. }) => items,
            _ => Vec::new(),
        };
        return Err(Error::LocalChanges {
            id: except.into(),
            message: format!(
                "the shared clone {relative} must move to the installed revision of {target_id}, but its local changes conflict with that revision; commit, stash or discard them first ({error})"
            ),
            items,
        });
    }
    Ok(())
}

/// What uninstalling `id` removes: its clone unless other packages (registered or
/// forgotten) still use it (then `repo` is `None` and `key` names the shared clone whose
/// HEAD may have to move), and the local work lost with that.
pub(super) struct Removal {
    pub(super) relative: String,
    pub(super) repo: Option<PathBuf>,
    pub(super) key: Option<String>,
    pub(super) lost_work: Vec<LostWork>,
}
pub(super) fn removal(root: &Path, state: &State, id: &str) -> Result<Removal> {
    let entry = state.packages.get(id).or_else(|| state.forgotten.get(id));
    let Some((p, key)) = entry.and_then(|p| Some((p, p.clone.clone()?))) else {
        // Own clone, registered or forgotten (retained) under `packages/<id>`.
        let repo = match package_repo(root, id) {
            Ok(repo) => repo,
            // A record (forgotten or not) whose clone is gone: only the record (and dist)
            // remain.
            Err(Error::RepositoryNotFound { .. }) if entry.is_some() => {
                return Ok(Removal {
                    relative: format!("packages/{id}"),
                    repo: None,
                    key: None,
                    lost_work: Vec::new(),
                });
            }
            Err(error) => return Err(error),
        };
        let recorded = state.packages.get(id).map(|p| p.current.commit.as_str());
        return Ok(Removal {
            relative: format!("packages/{id}"),
            lost_work: lost_work::detect(&repo, recorded)?,
            repo: Some(repo),
            key: None,
        });
    };
    let relative = p.clone_folder(id);
    let live = match installed_repo(root, id, p) {
        Ok(live) => live,
        // A record (forgotten or not) whose shared clone is gone: only the record (and
        // dist) remain.
        Err(Error::RepositoryNotFound { .. }) => {
            return Ok(Removal {
                relative,
                repo: None,
                key: None,
                lost_work: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };
    let others = clone_users(state, &key, id);
    if !others.is_empty() {
        // HEAD moves to another user's revision: commits only HEAD holds would be lost.
        let head = git(&live, &["rev-parse", "HEAD"])?;
        let keep: Vec<&str> = others
            .iter()
            .map(|(_, p)| p.current.commit.as_str())
            .collect();
        let mut lost_work = Vec::new();
        if !keep.contains(&head.as_str()) {
            lost_work = lost_work::unreferenced_head_commits(&live, &keep)?;
        }
        return Ok(Removal {
            relative,
            repo: None,
            key: Some(key),
            lost_work,
        });
    }
    let mut lost_work = lost_work::detect(&live, None)?;
    if let Some(item) = shared_head_moved(&live, state, &key, id)? {
        lost_work.insert(0, item);
    }
    Ok(Removal {
        relative,
        repo: Some(live),
        key: Some(key),
        lost_work,
    })
}
