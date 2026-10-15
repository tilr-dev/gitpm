//! Detection of local work that deleting or replacing a plugin clone would lose.
use crate::error::{Error, Result};
use crate::exec::{git, git_raw};
use serde::Serialize;
use std::path::Path;

/// Upper bound of listed unpublished commits per plan (the check itself is exact).
const MAX_COMMITS: &str = "--max-count=50";

/// One item of local work in a plugin clone.
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

const DIRTY: &str =
    "plugin has local changes (including untracked/ignored files); keep or remove them first";

/// Modified, untracked and ignored files (`git status`, without taking optional locks).
fn status(repo: &Path) -> Result<Vec<LostWork>> {
    let raw = git_raw(
        repo,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored",
        ],
    )?;
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
pub(crate) fn clean(repo: &Path, id: &str) -> Result<()> {
    let items = status(repo)?;
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
pub(crate) fn require_head(repo: &Path, id: &str, recorded: &str, message: &str) -> Result<()> {
    let head = git(repo, &["rev-parse", "HEAD"])?;
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

fn unpublished_refs(repo: &Path, prefix: &str) -> Result<Vec<(String, u64)>> {
    let mut result = Vec::new();
    for name in git(repo, &["for-each-ref", "--format=%(refname)", prefix])?.lines() {
        let count: u64 = git(repo, &["rev-list", "--count", name, "--not", "--remotes"])?
            .parse()
            .map_err(|e| Error::Git(format!("git rev-list --count: {e}")))?;
        if count > 0 {
            let short = name.strip_prefix(prefix).unwrap_or(name).to_owned();
            result.push((short, count));
        }
    }
    Ok(result)
}

/// Every kind of local work in `repo`. `recorded` is the installed commit from state
/// (`None` for a forgotten repository).
pub(crate) fn detect(repo: &Path, recorded: Option<&str>) -> Result<Vec<LostWork>> {
    let mut items = status(repo)?;
    let head = git(repo, &["rev-parse", "HEAD"])?;
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
    )?;
    for line in log.lines() {
        let (commit, summary) = line.split_once('\t').unwrap_or((line, ""));
        items.push(LostWork::UnpublishedCommit {
            commit: commit.into(),
            summary: summary.into(),
        });
    }
    for (name, commits) in unpublished_refs(repo, "refs/heads/")? {
        items.push(LostWork::UnpublishedBranch { name, commits });
    }
    for (name, _) in unpublished_refs(repo, "refs/tags/")? {
        items.push(LostWork::UnpublishedTag { name });
    }
    for entry in git(repo, &["stash", "list"])?.lines() {
        items.push(LostWork::Stash {
            entry: entry.into(),
        });
    }
    Ok(items)
}

/// `LocalChanges` with the message of the most important category (as before the split).
pub(crate) fn refusal(id: &str, items: Vec<LostWork>) -> Error {
    let message = if items.iter().any(LostWork::is_file_change) {
        DIRTY
    } else if items.iter().any(|i| {
        matches!(
            i,
            LostWork::HeadMoved { .. } | LostWork::UnpublishedCommit { .. }
        )
    }) {
        "local commits detected; use --force to discard"
    } else if items.iter().any(|i| {
        matches!(
            i,
            LostWork::UnpublishedBranch { .. } | LostWork::UnpublishedTag { .. }
        )
    }) {
        "local branches/tags contain unpublished commits; use --force to discard"
    } else {
        "stash contains local work; use --force to discard"
    };
    Error::LocalChanges {
        id: id.into(),
        message: message.into(),
        items,
    }
}
