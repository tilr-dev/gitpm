//! Bare source repositories and independent package worktrees.
use super::*;

/// Prefix of the refs that keep the installed revisions of a shared clone reachable.
const KEEP: &str = "refs/gitpm/";

/// Make the `refs/gitpm/` refs of the shared clone `repo` match the users of `key` in
/// `state`: one ref per recorded current/previous revision, none for anything else (an
/// uninstalled package's refs go). A revision already missing from the clone is skipped.
pub(super) fn sync_refs(repo: &Path, state: &State, key: &str, custom_ssh: bool) -> Result<()> {
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
    for line in transport::refs_with_objects(repo, KEEP, custom_ssh)?.lines() {
        if let Some((name, object)) = line.split_once(' ') {
            have.insert(name.to_owned(), object.to_owned());
        }
    }
    for name in have.keys().filter(|name| !wanted.contains_key(*name)) {
        transport::delete_ref(repo, name, custom_ssh)?;
    }
    for (name, commit) in &wanted {
        if have.get(name) == Some(commit)
            || transport::require_object(repo, &format!("{commit}^{{commit}}"), custom_ssh).is_err()
        {
            continue;
        }
        transport::update_ref(repo, name, commit, custom_ssh)?;
    }
    Ok(())
}

/// Fetch the origin's tags into a shared clone as they are there (a moved tag is taken
/// at its new commit; with `prune`, deleted ones go), after protecting every installed
/// revision with [`sync_refs`]. Tags are not policed: installed revisions are kept by
/// commit.
pub(super) fn fetch_tags(
    repo: &Path,
    state: &State,
    key: &str,
    prune: bool,
    custom_ssh: bool,
) -> Result<()> {
    sync_refs(repo, state, key, custom_ssh)?;
    fetch_origin(repo, prune, custom_ssh)
}

/// `git fetch origin --force --tags` (with `prune`, also `--prune --prune-tags`, so the
/// clone mirrors the origin's tags), then record those tags as the origin's tags of the
/// last fetch ([`lost_work::record_origin_tags`]).
pub(super) fn fetch_origin(repo: &Path, prune: bool, custom_ssh: bool) -> Result<()> {
    crate::transport::fetch(
        repo,
        crate::transport::Fetch::Tags {
            remote: "origin",
            prune,
        },
        custom_ssh,
    )?;
    lost_work::record_origin_tags(repo, custom_ssh)
}

/// Cache key of the shared clone of a source, its folder under `repos/`:
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

/// The source object store, independent of package worktree lifetime.
pub(super) fn source_repo(root: &Path, id: &str, p: &Installed) -> Result<PathBuf> {
    if p.link().is_some() {
        return Err(Error::RepositoryNotFound { id: id.into() });
    }
    let key = p
        .clone
        .as_ref()
        .ok_or_else(|| Error::RepositoryNotFound { id: id.into() })?;
    txn::ensure_managed_parents(root, &format!("repos/{key}"))?;
    let repo = root.join("repos").join(key);
    if !exact_path(&root.join("repos"), key)?
        || !repo.join("HEAD").is_file()
        || !repo.join("objects").is_dir()
    {
        return Err(Error::RepositoryNotFound { id: id.into() });
    }
    Ok(repo)
}

/// Git installations own a worktree; builds may retain only their source objects.
pub(super) fn installed_repo(root: &Path, id: &str, p: &Installed) -> Result<PathBuf> {
    let bare = source_repo(root, id, p)?;
    if p.build.is_some() {
        return Ok(bare);
    }
    txn::ensure_managed_parents(root, &format!("worktrees/{id}"))?;
    let repo = root.join("worktrees").join(id);
    if !exact_path(&root.join("worktrees"), id)? || !repo.join(".git").is_file() {
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

/// Local-work guard before an operation copies and changes a clone: no file changes,
/// HEAD at this package's recorded revision, no
/// unpublished branches/tags/stash.
pub(super) fn guard_clone(
    live: &Path,
    state: &State,
    id: &str,
    head_message: &str,
    hidden_message: &str,
    for_update: bool,
    custom_ssh: bool,
) -> Result<()> {
    let package = state
        .packages
        .get(id)
        .or_else(|| state.forgotten.get(id))
        .ok_or_else(|| Error::RepositoryNotFound { id: id.into() })?;
    if package.build.is_some() {
        return Ok(());
    }
    clean(live, id, custom_ssh)?;
    lost_work::require_head(live, id, &package.current.commit, head_message, custom_ssh)?;
    if for_update {
        lost_work::require_no_hidden_for_update(live, id, hidden_message, custom_ssh)
    } else {
        lost_work::require_no_hidden(live, id, hidden_message, custom_ssh)
    }
}

/// The URL `origin` is configured with, as written (`git remote get-url` would apply
/// `url.<base>.insteadOf` rewrites and no longer match the recorded source).
pub(super) fn configured_origin(repo: &Path, custom_ssh: bool) -> Option<String> {
    transport::config_get(repo, "remote.origin.url", custom_ssh).ok()
}

/// Adopt/reuse the bare source and create only this package's detached worktree.
pub(super) fn place_clone(
    txn: &mut Txn,
    state: &State,
    cloned: &Cloned,
    source: &str,
    custom_ssh: bool,
) -> Result<(PathBuf, String, Option<String>)> {
    let id = &cloned.manifest.id;
    let key = repo_key(source);
    let relative = format!("repos/{key}");
    let live = txn.root().join(&relative);
    let staged = if let Some(staged) = txn.staged_repo(&relative) {
        staged
    } else if fs::symlink_metadata(&live).is_ok() {
        ensure!(
            live.join("HEAD").is_file()
                && live.join("objects").is_dir()
                && configured_origin(&live, custom_ssh).as_deref() == Some(source),
            Workspace,
            "{relative} exists but is not the bare repository of {source}"
        );
        txn.repo(&relative, &live)?
    } else {
        txn.add_repo(&relative, &cloned.repo)?
    };
    // Reused objects can already exist while origin refs have changed.
    if live.exists() || transport::require_object(&staged, &cloned.commit, custom_ssh).is_err() {
        let mut overwritten = Vec::new();
        for item in lost_work::hidden(&staged, false, custom_ssh)? {
            if let LostWork::UnpublishedTag { name } = &item {
                let reference = format!("refs/tags/{name}");
                let local = transport::object_id(&staged, &reference, custom_ssh)?;
                if transport::verified_object_id(&cloned.repo, &reference, custom_ssh)
                    .is_ok_and(|incoming| incoming != local)
                {
                    overwritten.push(item);
                }
            }
        }
        if !overwritten.is_empty() {
            return Err(lost_work::refusal(id, overwritten));
        }
        fetch_tags(&staged, state, &key, false, custom_ssh)?;
    }
    let worktree = txn.worktree(id, &relative, &cloned.commit, custom_ssh)?;
    let m = package_manifest(&worktree, cloned.path.as_deref())?;
    ensure!(
        serde_json::to_value(&m)? == serde_json::to_value(&cloned.manifest)?,
        VersionConflict,
        "package {id} differs between the bare repository and its source"
    );
    Ok((worktree, relative, Some(key)))
}

/// What uninstalling `id` removes: its worktree, and the source repository only
/// when no installed or forgotten package still needs it, with any lost work.
pub(super) struct Removal {
    pub(super) relative: String,
    pub(super) repo: Option<PathBuf>,
    pub(super) key: Option<String>,
    pub(super) lost_work: Vec<LostWork>,
}
pub(super) fn removal(root: &Path, state: &State, id: &str, custom_ssh: bool) -> Result<Removal> {
    let entry = state.packages.get(id).or_else(|| state.forgotten.get(id));
    if entry.is_some_and(|p| p.link().is_some()) {
        return Ok(Removal {
            relative: String::new(),
            repo: None,
            key: None,
            lost_work: Vec::new(),
        });
    }
    let p = entry.ok_or_else(|| Error::RepositoryNotFound { id: id.into() })?;
    let relative = p.clone_folder(id);
    let bare = match source_repo(root, id, p) {
        Ok(bare) => Some(bare),
        Err(Error::RepositoryNotFound { .. }) => None,
        Err(error) => return Err(error),
    };
    // Without its source store, the orphan checkout may contain unrecoverable work.
    let key = bare.as_ref().and(p.clone.clone());
    let repo = if key
        .as_ref()
        .is_some_and(|key| clone_users(state, key, id).is_empty())
    {
        bare.clone()
    } else {
        None
    };
    let mut lost_work = Vec::new();
    if p.build.is_none() {
        match installed_repo(root, id, p) {
            Ok(worktree) => {
                lost_work = if repo.is_some() {
                    lost_work::detect(&worktree, Some(&p.current.commit), custom_ssh)?
                } else {
                    lost_work::worktree_removal(&worktree, id, &p.current.commit, custom_ssh)?
                };
            }
            Err(Error::RepositoryNotFound { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    if let Some(bare) = &repo {
        for item in lost_work::hidden(bare, false, custom_ssh)? {
            if !lost_work.contains(&item) {
                lost_work.push(item);
            }
        }
    }
    Ok(Removal {
        relative,
        repo,
        key,
        lost_work,
    })
}
