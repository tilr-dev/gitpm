//! Detection of local work that deleting or replacing a package clone would lose.
use crate::error::{Error, Result};
use crate::exec::{git, git_raw};
use serde::Serialize;
use std::path::Path;

/// Upper bound of listed unpublished commits per plan (the check itself is exact).
const MAX_COMMITS: &str = "--max-count=50";

/// One item of local work in a package clone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LostWork {
    /// Tracked file modified, added, deleted or renamed (staged or not).
    Modified {
        path: String,
    },
    Untracked {
        path: String,
    },
    Ignored {
        path: String,
    },
    /// HEAD is not the installed revision recorded in state.
    HeadMoved {
        head: String,
        recorded: String,
    },
    /// Commit reachable from HEAD but from no remote-tracking branch.
    UnpublishedCommit {
        commit: String,
        summary: String,
    },
    /// Local branch with commits that no remote-tracking branch contains.
    UnpublishedBranch {
        name: String,
        commits: u64,
    },
    /// Local tag pointing to commits that no remote-tracking branch contains.
    UnpublishedTag {
        name: String,
    },
    /// `git stash` entry.
    Stash {
        entry: String,
    },
}
impl LostWork {
    /// One-line human description.
    pub fn describe(&self) -> String {
        match self {
            LostWork::Modified { path } => format!("modified: {path}"),
            LostWork::Untracked { path } => format!("untracked: {path}"),
            LostWork::Ignored { path } => format!("ignored: {path}"),
            LostWork::HeadMoved { head, recorded } => {
                format!("HEAD moved: {head} (installed {recorded})")
            }
            LostWork::UnpublishedCommit { commit, summary } => {
                format!("unpublished commit: {commit} {summary}")
            }
            LostWork::UnpublishedBranch { name, commits } => {
                format!("unpublished branch: {name} ({commits} commit(s))")
            }
            LostWork::UnpublishedTag { name } => format!("unpublished tag: {name}"),
            LostWork::Stash { entry } => format!("stash: {entry}"),
        }
    }
    fn is_file_change(&self) -> bool {
        matches!(
            self,
            LostWork::Modified { .. } | LostWork::Untracked { .. } | LostWork::Ignored { .. }
        )
    }
}

const DIRTY: &str = "package has local changes (including untracked/ignored files); commit or discard the local changes first";
/// [`refusal`]: an uninstall removes the listed lost work once it is confirmed.
const CONFIRM: &str = "confirm the listed lost work to remove it";

/// Modified, untracked and ignored files (`git status`, without taking optional locks).
fn status(repo: &Path, custom_ssh: bool) -> Result<Vec<LostWork>> {
    status_of(repo, true, &[], custom_ssh)
}

/// Paths under `paths` (pathspecs) that differ from HEAD: modified, staged or untracked
/// (whatever `status.showUntrackedFiles` says), not ignored ones.
pub(crate) fn changed_paths(repo: &Path, paths: &[&str], custom_ssh: bool) -> Result<Vec<String>> {
    Ok(status_of(repo, false, paths, custom_ssh)?
        .into_iter()
        .filter_map(|item| match item {
            LostWork::Modified { path } | LostWork::Untracked { path } => Some(path),
            _ => None,
        })
        .collect())
}

fn status_of(
    repo: &Path,
    ignored: bool,
    paths: &[&str],
    custom_ssh: bool,
) -> Result<Vec<LostWork>> {
    let mut args = vec![
        "--no-optional-locks",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ];
    if ignored {
        args.push("--ignored");
    }
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths);
    }
    let raw = git_raw(repo, &args, custom_ssh)?;
    let mut items = Vec::new();
    let mut entries = raw.split(|b| *b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        match code {
            b"??" => items.push(LostWork::Untracked { path }),
            b"!!" => items.push(LostWork::Ignored { path }),
            _ => {
                // Renames and copies are followed by the original path.
                if matches!(code[0], b'R' | b'C') || matches!(code[1], b'R' | b'C') {
                    entries.next();
                }
                items.push(LostWork::Modified { path });
            }
        }
    }
    Ok(items)
}

/// Refuse when the clone has modified, untracked or ignored files.
pub(crate) fn clean(repo: &Path, id: &str, custom_ssh: bool) -> Result<()> {
    let items = status(repo, custom_ssh)?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::LocalChanges {
        id: id.into(),
        message: DIRTY.into(),
        items,
    })
}

/// Refuse when HEAD differs from the recorded installed commit.
pub(crate) fn require_head(
    repo: &Path,
    id: &str,
    recorded: &str,
    message: &str,
    custom_ssh: bool,
) -> Result<()> {
    let head = git(repo, &["rev-parse", "HEAD"], custom_ssh)?;
    if head == recorded {
        return Ok(());
    }
    Err(Error::LocalChanges {
        id: id.into(),
        message: message.into(),
        items: vec![LostWork::HeadMoved {
            head,
            recorded: recorded.into(),
        }],
    })
}

/// Where a clone records the origin's tags as of its last fetch (see
/// [`record_origin_tags`]), so an offline check knows which tags are published.
pub(crate) const ORIGIN_TAGS: &str = "refs/origin-tags/";

/// Record the origin's tags under [`ORIGIN_TAGS`], read from the origin itself (never
/// from local tags, so a tag made by hand never counts as published), pruning records of
/// tags the origin deleted. Called after every clone and every fetch from the origin.
pub(crate) fn record_origin_tags(repo: &Path, custom_ssh: bool) -> Result<()> {
    git(
        repo,
        &[
            "fetch",
            "--quiet",
            "--prune",
            "--no-tags",
            "origin",
            &format!("+refs/tags/*:{ORIGIN_TAGS}*"),
        ],
        custom_ssh,
    )?;
    Ok(())
}

fn unpublished_refs(repo: &Path, prefix: &str, custom_ssh: bool) -> Result<Vec<(String, u64)>> {
    let mut result = Vec::new();
    for name in git(
        repo,
        &["for-each-ref", "--format=%(refname)", prefix],
        custom_ssh,
    )?
    .lines()
    {
        let count: u64 = git(
            repo,
            &["rev-list", "--count", name, "--not", "--remotes"],
            custom_ssh,
        )?
        .parse()
        .map_err(|e| Error::Git(format!("git rev-list --count: {e}")))?;
        if count > 0 {
            let short = name.strip_prefix(prefix).unwrap_or(name).to_owned();
            result.push((short, count));
        }
    }
    Ok(result)
}

/// Local branches and tags with commits no remote-tracking branch contains, and stash
/// entries: work that survives a checkout but is easy to lose track of.
fn hidden(repo: &Path, check_remote: bool, custom_ssh: bool) -> Result<Vec<LostWork>> {
    let mut items = Vec::new();
    for (name, commits) in unpublished_refs(repo, "refs/heads/", custom_ssh)? {
        items.push(LostWork::UnpublishedBranch { name, commits });
    }
    let tags = unpublished_refs(repo, "refs/tags/", custom_ssh)?;
    // A published tag need not be reachable from any remote branch (forks commonly
    // retain upstream tags): a tag the origin had with the same object at the last
    // fetch is published (offline); `check_remote` also accepts one it has now.
    let published = if tags.is_empty() || !check_remote {
        String::new()
    } else {
        git(repo, &["ls-remote", "--tags", "origin"], custom_ssh)?
    };
    for (name, _) in tags {
        let reference = format!("refs/tags/{name}");
        let object = git(repo, &["rev-parse", &reference], custom_ssh)?;
        let fetched = git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "-q",
                &format!("{ORIGIN_TAGS}{name}"),
            ],
            custom_ssh,
        )
        .is_ok_and(|seen| seen == object);
        if !fetched
            && !published
                .lines()
                .any(|line| line == format!("{object}\t{reference}"))
        {
            items.push(LostWork::UnpublishedTag { name });
        }
    }
    for entry in git(repo, &["stash", "list"], custom_ssh)?.lines() {
        items.push(LostWork::Stash {
            entry: entry.into(),
        });
    }
    Ok(items)
}

/// Updating already needs the origin; account for tags published outside branches.
pub(crate) fn require_no_hidden_for_update(
    repo: &Path,
    id: &str,
    message: &str,
    custom_ssh: bool,
) -> Result<()> {
    let items = hidden(repo, true, custom_ssh)?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::LocalChanges {
        id: id.into(),
        message: message.into(),
        items,
    })
}

/// Refuse when local branches/tags hold unpublished commits or the stash is not empty.
pub(crate) fn require_no_hidden(
    repo: &Path,
    id: &str,
    message: &str,
    custom_ssh: bool,
) -> Result<()> {
    let items = hidden(repo, false, custom_ssh)?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::LocalChanges {
        id: id.into(),
        message: message.into(),
        items,
    })
}

/// Every kind of local work in `repo`. `recorded` is the installed commit from state
/// (`None` for a forgotten repository).
pub(crate) fn detect(
    repo: &Path,
    recorded: Option<&str>,
    custom_ssh: bool,
) -> Result<Vec<LostWork>> {
    let mut items = status(repo, custom_ssh)?;
    let head = git(repo, &["rev-parse", "HEAD"], custom_ssh)?;
    if let Some(recorded) = recorded
        && head != recorded
    {
        items.push(LostWork::HeadMoved {
            head: head.clone(),
            recorded: recorded.into(),
        });
    }
    let log = git(
        repo,
        &[
            "log",
            MAX_COMMITS,
            "--format=%H%x09%s",
            "HEAD",
            "--not",
            "--remotes",
        ],
        custom_ssh,
    )?;
    for line in log.lines() {
        let (commit, summary) = line.split_once('\t').unwrap_or((line, ""));
        items.push(LostWork::UnpublishedCommit {
            commit: commit.into(),
            summary: summary.into(),
        });
    }
    items.extend(hidden(repo, false, custom_ssh)?);
    Ok(items)
}

/// Commits reachable from HEAD that no remote-tracking branch and no tag contains:
/// work only this clone has (a restore would record it as if it were published).
pub(crate) fn unpublished_head_commits(repo: &Path, custom_ssh: bool) -> Result<Vec<LostWork>> {
    Ok(git(
        repo,
        &[
            "log",
            MAX_COMMITS,
            "--format=%H%x09%s",
            "HEAD",
            "--not",
            "--remotes",
            "--tags",
        ],
        custom_ssh,
    )?
    .lines()
    .map(|line| {
        let (commit, summary) = line.split_once('\t').unwrap_or((line, ""));
        LostWork::UnpublishedCommit {
            commit: commit.into(),
            summary: summary.into(),
        }
    })
    .collect())
}

/// Commits that moving HEAD away would leave unreachable: reachable from HEAD but from
/// no branch, tag, remote-tracking ref or `keep` commit.
pub(crate) fn unreferenced_head_commits(
    repo: &Path,
    keep: &[&str],
    custom_ssh: bool,
) -> Result<Vec<LostWork>> {
    let mut args = vec![
        "log",
        MAX_COMMITS,
        "--format=%H%x09%s",
        "HEAD",
        "--not",
        "--branches",
        "--tags",
        "--remotes",
    ];
    args.extend(keep.iter().copied());
    Ok(git(repo, &args, custom_ssh)?
        .lines()
        .map(|line| {
            let (commit, summary) = line.split_once('\t').unwrap_or((line, ""));
            LostWork::UnpublishedCommit {
                commit: commit.into(),
                summary: summary.into(),
            }
        })
        .collect())
}

/// `LocalChanges` with the message of the most important category (as before the split).
pub(crate) fn refusal(id: &str, items: Vec<LostWork>) -> Error {
    let message = if items.iter().any(LostWork::is_file_change) {
        "package has local changes (including untracked/ignored files)"
    } else if items.iter().any(|i| {
        matches!(
            i,
            LostWork::HeadMoved { .. } | LostWork::UnpublishedCommit { .. }
        )
    }) {
        "local commits detected"
    } else if items.iter().any(|i| {
        matches!(
            i,
            LostWork::UnpublishedBranch { .. } | LostWork::UnpublishedTag { .. }
        )
    }) {
        "local branches/tags contain unpublished commits"
    } else {
        "stash contains local work"
    };
    Error::LocalChanges {
        id: id.into(),
        message: format!("{message}; {CONFIRM}"),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir, git as fixture_git};
    use std::{fs, path::PathBuf};

    #[test]
    fn describe_every_variant() {
        let cases = [
            (
                LostWork::Modified {
                    path: "a.js".into(),
                },
                "modified: a.js",
            ),
            (LostWork::Untracked { path: "b c".into() }, "untracked: b c"),
            (
                LostWork::Ignored {
                    path: "node_modules/x".into(),
                },
                "ignored: node_modules/x",
            ),
            (
                LostWork::HeadMoved {
                    head: "h".into(),
                    recorded: "r".into(),
                },
                "HEAD moved: h (installed r)",
            ),
            (
                LostWork::UnpublishedCommit {
                    commit: "c".into(),
                    summary: "fix".into(),
                },
                "unpublished commit: c fix",
            ),
            (
                LostWork::UnpublishedBranch {
                    name: "topic".into(),
                    commits: 2,
                },
                "unpublished branch: topic (2 commit(s))",
            ),
            (
                LostWork::UnpublishedTag { name: "t".into() },
                "unpublished tag: t",
            ),
            (
                LostWork::Stash {
                    entry: "stash@{0}: WIP".into(),
                },
                "stash: stash@{0}: WIP",
            ),
        ];
        for (item, text) in cases {
            assert_eq!(item.describe(), text);
        }
    }

    #[test]
    fn serialized_with_snake_case_kind_tag() {
        let value = serde_json::to_value(LostWork::HeadMoved {
            head: "h".into(),
            recorded: "r".into(),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"kind": "head_moved", "head": "h", "recorded": "r"})
        );
        let value = serde_json::to_value(LostWork::UnpublishedBranch {
            name: "b".into(),
            commits: 3,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"kind": "unpublished_branch", "name": "b", "commits": 3})
        );
    }

    #[test]
    fn refusal_message_follows_category_priority() {
        let file = LostWork::Ignored { path: "x".into() };
        let commit = LostWork::UnpublishedCommit {
            commit: "c".into(),
            summary: "s".into(),
        };
        let head = LostWork::HeadMoved {
            head: "h".into(),
            recorded: "r".into(),
        };
        let tag = LostWork::UnpublishedTag { name: "t".into() };
        let branch = LostWork::UnpublishedBranch {
            name: "b".into(),
            commits: 1,
        };
        let stash = LostWork::Stash { entry: "s".into() };
        let message = |items: Vec<LostWork>| {
            let error = refusal("dan/demo1", items.clone());
            match error {
                Error::LocalChanges {
                    id,
                    message,
                    items: kept,
                } => {
                    assert_eq!(id, "dan/demo1");
                    assert_eq!(kept, items, "items are kept unchanged");
                    message
                }
                other => panic!("unexpected {other:?}"),
            }
        };
        assert_eq!(
            message(vec![stash.clone(), tag.clone(), commit.clone(), file]),
            "package has local changes (including untracked/ignored files); confirm the listed lost work to remove it"
        );
        assert_eq!(
            message(vec![stash.clone(), branch.clone(), commit]),
            "local commits detected; confirm the listed lost work to remove it"
        );
        assert_eq!(
            message(vec![head]),
            "local commits detected; confirm the listed lost work to remove it"
        );
        assert_eq!(
            message(vec![stash.clone(), tag]),
            "local branches/tags contain unpublished commits; confirm the listed lost work to remove it"
        );
        assert_eq!(
            message(vec![branch]),
            "local branches/tags contain unpublished commits; confirm the listed lost work to remove it"
        );
        assert_eq!(
            message(vec![stash]),
            "stash contains local work; confirm the listed lost work to remove it"
        );
    }

    /// A clone of a tagged author repository (has `origin`, like an installed package).
    fn clone(dir: &TempDir) -> PathBuf {
        let author = Author::new(dir.join("author"), "dan/demo1");
        fixture_git(dir.path(), &["clone", "-q", author.source(), "clone"]);
        dir.join("clone")
    }

    #[test]
    fn clean_clone_has_no_lost_work() {
        let dir = TempDir::new("lost-work");
        let repo = clone(&dir);
        let head = fixture_git(&repo, &["rev-parse", "HEAD"]);
        assert!(status(&repo, false).unwrap().is_empty());
        assert!(clean(&repo, "dan/demo1", false).is_ok());
        assert!(require_head(&repo, "dan/demo1", &head, "m", false).is_ok());
        assert_eq!(detect(&repo, Some(&head), false).unwrap(), []);
        assert_eq!(detect(&repo, None, false).unwrap(), []);
    }

    #[test]
    fn status_parses_modified_staged_renamed_untracked_and_ignored_paths() {
        let dir = TempDir::new("lost-work");
        let repo = clone(&dir);
        fs::write(repo.join(".gitignore"), "*.log\n").unwrap();
        fixture_git(&repo, &["add", ".gitignore"]);
        fixture_git(&repo, &["commit", "-q", "-m", "ignore"]);
        fs::write(repo.join("tracked file.txt"), "x").unwrap();
        fixture_git(&repo, &["add", "tracked file.txt"]);
        fixture_git(&repo, &["commit", "-q", "-m", "more"]);

        fixture_git(&repo, &["mv", "tracked file.txt", "renamed файл.txt"]);
        fs::write(repo.join("dist/index.js"), "edited").unwrap();
        fs::write(repo.join("staged.txt"), "new").unwrap();
        fixture_git(&repo, &["add", "staged.txt"]);
        fs::remove_file(repo.join("plugin.json")).unwrap();
        fs::create_dir_all(repo.join("new/deep")).unwrap();
        fs::write(repo.join("new/deep/u.txt"), "u").unwrap();
        fs::write(repo.join("debug.log"), "l").unwrap();

        let mut items = status(&repo, false).unwrap();
        items.sort_by_key(|i| i.describe());
        let described: Vec<_> = items.iter().map(LostWork::describe).collect();
        assert_eq!(
            described,
            [
                "ignored: debug.log",
                "modified: dist/index.js",
                "modified: plugin.json",
                "modified: renamed файл.txt",
                "modified: staged.txt",
                "untracked: new/deep/u.txt",
            ]
        );
        let error = clean(&repo, "dan/demo1", false).unwrap_err();
        assert_eq!(error.kind(), "local_changes");
        assert_eq!(error.to_string(), DIRTY);
        assert!(matches!(error, Error::LocalChanges { items, .. } if items.len() == 6));
    }

    #[test]
    fn head_moves_commits_branches_tags_and_stash_are_detected() {
        let dir = TempDir::new("lost-work");
        let repo = clone(&dir);
        let installed = fixture_git(&repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("dist/index.js"), "local").unwrap();
        fixture_git(&repo, &["commit", "-q", "-am", "local\twork"]);
        let local = fixture_git(&repo, &["rev-parse", "HEAD"]);
        fixture_git(&repo, &["tag", "mine"]);
        fixture_git(&repo, &["branch", "topic"]);
        fs::write(repo.join("dist/index.js"), "stashed").unwrap();
        fixture_git(&repo, &["stash", "-q"]);

        let error = require_head(&repo, "dan/demo1", &installed, "moved", false).unwrap_err();
        assert_eq!(error.to_string(), "moved");
        let Error::LocalChanges { items, .. } = error else {
            panic!()
        };
        assert_eq!(
            items,
            [LostWork::HeadMoved {
                head: local.clone(),
                recorded: installed.clone()
            }]
        );

        let items = detect(&repo, Some(&installed), false).unwrap();
        let described: Vec<_> = items.iter().map(LostWork::describe).collect();
        assert_eq!(described.len(), 6, "{described:?}");
        assert_eq!(
            described[0],
            format!("HEAD moved: {local} (installed {installed})")
        );
        // Only the first tab separates commit and summary.
        assert_eq!(
            described[1],
            format!("unpublished commit: {local} local\twork")
        );
        assert_eq!(described[2], "unpublished branch: main (1 commit(s))");
        assert_eq!(described[3], "unpublished branch: topic (1 commit(s))");
        assert_eq!(described[4], "unpublished tag: mine");
        assert!(
            described[5].starts_with("stash: stash@{0}"),
            "{}",
            described[5]
        );
        // A forgotten repository (no recorded commit) has no HeadMoved item.
        let items = detect(&repo, None, false).unwrap();
        assert!(
            !items
                .iter()
                .any(|i| matches!(i, LostWork::HeadMoved { .. }))
        );
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn detached_checkout_of_a_published_commit_is_only_a_head_move() {
        let dir = TempDir::new("lost-work");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let first = author.head();
        author.release("0.2.0");
        fixture_git(dir.path(), &["clone", "-q", author.source(), "clone"]);
        let repo = dir.join("clone");
        fixture_git(&repo, &["checkout", "-q", "--detach", &first]);
        let second = author.head();
        assert_eq!(
            detect(&repo, Some(&second), false).unwrap(),
            [LostWork::HeadMoved {
                head: first,
                recorded: second
            }]
        );
    }

    #[test]
    fn unpublished_commit_listing_is_bounded() {
        let dir = TempDir::new("lost-work");
        let repo = clone(&dir);
        for n in 0..55 {
            fixture_git(
                &repo,
                &["commit", "-q", "--allow-empty", "-m", &format!("c{n}")],
            );
        }
        let items = detect(&repo, None, false).unwrap();
        let commits = items
            .iter()
            .filter(|i| matches!(i, LostWork::UnpublishedCommit { .. }))
            .count();
        assert_eq!(commits, 50);
        // The branch count stays exact.
        assert!(items.contains(&LostWork::UnpublishedBranch {
            name: "main".into(),
            commits: 55
        }));
    }
}
