//! Replaceable, bounded Git and HTTP transport.
mod system;
use crate::error::Result;
use std::path::Path;
pub(crate) use system::{LsRemote, RemoteRefs};

/// Fetch operations without command-line flags.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Fetch<'a> {
    /// Fetch a specific missing revision from a remote.
    Revision { remote: &'a str, revision: &'a str },
    /// Force-update tags, optionally pruning deleted origin tags.
    Tags { remote: &'a str, prune: bool },
    /// Mirror remote tags into a private local ref namespace.
    RecordTags { remote: &'a str, namespace: &'a str },
}
/// Reachability rules for local HEAD history diagnostics.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HeadHistory<'a> {
    Unpublished { exclude_tags: bool },
    Unreferenced { keep: &'a [&'a str] },
}

/// External Git/HTTP operations. Raw local Git remains for staged migration.
pub(crate) trait Transport {
    /// Query an object expression (including revision/path and peel expressions).
    fn object_type(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<String>;
    /// Return an error when an object expression cannot be resolved.
    fn require_object(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<()>;
    /// Read blob bytes without text decoding or trimming.
    fn read_blob(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<Vec<u8>>;
    /// Read recursive NUL-terminated tree records, optionally including trees.
    fn list_tree(
        &self,
        repo: &Path,
        tree: &str,
        include_trees: bool,
        custom_ssh: bool,
    ) -> Result<Vec<u8>>;
    /// Resolve a revision, optionally suppressing Git's missing-object diagnostic.
    fn verify_revision(
        &self,
        repo: &Path,
        revision: &str,
        quiet: bool,
        custom_ssh: bool,
    ) -> Result<String>;
    /// Return an error unless `ancestor` is reachable from `descendant`.
    fn require_ancestor(
        &self,
        repo: &Path,
        ancestor: &str,
        descendant: &str,
        custom_ssh: bool,
    ) -> Result<()>;
    fn checkout_detached(
        &self,
        repo: &Path,
        revision: &str,
        quiet: bool,
        custom_ssh: bool,
    ) -> Result<()>;
    /// Read `refname objectname` lines beneath a ref prefix.
    fn refs_with_objects(&self, repo: &Path, prefix: &str, custom_ssh: bool) -> Result<String>;
    fn delete_ref(&self, repo: &Path, name: &str, custom_ssh: bool) -> Result<()>;
    fn update_ref(&self, repo: &Path, name: &str, object: &str, custom_ssh: bool) -> Result<()>;
    fn tags_at(&self, repo: &Path, revision: &str, custom_ssh: bool) -> Result<String>;
    fn config_get(&self, repo: &Path, key: &str, custom_ssh: bool) -> Result<String>;
    /// Read a config blob as NUL-delimited entries, each with a newline between key and value.
    fn config_blob(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<String>;
    fn config_file_add(
        &self,
        repo: &Path,
        file: &str,
        key: &str,
        value: &str,
        custom_ssh: bool,
    ) -> Result<()>;
    fn config_set(&self, repo: &Path, key: &str, value: &str, custom_ssh: bool) -> Result<()>;
    fn rename_remote(&self, repo: &Path, old: &str, new: &str, custom_ssh: bool) -> Result<()>;
    fn branches_matching(&self, repo: &Path, pattern: &str, custom_ssh: bool) -> Result<String>;
    /// Create and check out a branch at `start`, quietly.
    fn create_branch(&self, repo: &Path, branch: &str, start: &str, custom_ssh: bool)
    -> Result<()>;
    /// Remove a path recursively from the index, tolerating unmatched paths.
    fn remove_cached_path(&self, repo: &Path, path: &str, custom_ssh: bool) -> Result<()>;
    fn checkout_path(
        &self,
        repo: &Path,
        revision: &str,
        path: &str,
        custom_ssh: bool,
    ) -> Result<()>;
    fn stage_all(&self, repo: &Path, custom_ssh: bool) -> Result<()>;
    /// Commit the index quietly, with hooks disabled for gitpm-created forks.
    fn commit_without_hooks(&self, repo: &Path, message: &str, custom_ssh: bool) -> Result<()>;
    fn object_id(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String>;
    fn resolve_commit(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String>;
    fn verified_object_id(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String>;
    fn origin_default_branch(&self, repo: &Path, custom_ssh: bool) -> Result<String>;
    fn verify_ref(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<()>;
    fn show_file(&self, repo: &Path, commit: &str, path: &str, custom_ssh: bool) -> Result<String>;
    fn tree_paths(&self, repo: &Path, commit: &str, path: &str, custom_ssh: bool)
    -> Result<String>;
    fn tree_entries(&self, repo: &Path, commit: &str, custom_ssh: bool) -> Result<Vec<u8>>;
    fn tags(&self, repo: &Path, custom_ssh: bool) -> Result<String>;
    fn status(
        &self,
        repo: &Path,
        ignored: bool,
        paths: &[&str],
        custom_ssh: bool,
    ) -> Result<Vec<u8>>;
    fn refs(&self, repo: &Path, prefix: &str, custom_ssh: bool) -> Result<String>;
    fn unpublished_count(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String>;
    fn stashes(&self, repo: &Path, custom_ssh: bool) -> Result<String>;
    fn head_history(
        &self,
        repo: &Path,
        selection: HeadHistory<'_>,
        custom_ssh: bool,
    ) -> Result<String>;
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String>;
    fn git_with_input(
        &self,
        repo: &Path,
        args: &[&str],
        input: Vec<u8>,
        custom_ssh: bool,
    ) -> Result<()>;
    fn ls_remote(&self, source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote>;
    fn ls_remote_refs(&self, source: &str, custom_ssh: bool) -> Result<RemoteRefs>;
    fn http_get(&self, url: &str) -> Result<Option<Vec<u8>>>;
    fn clone_repo(
        &self,
        parent: &Path,
        source: &str,
        destination: &str,
        custom_ssh: bool,
    ) -> Result<()>;
    fn fetch(&self, repo: &Path, request: Fetch<'_>, custom_ssh: bool) -> Result<()>;
    fn set_remote_head(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<()>;
    fn remote_tags(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<String>;
}
/// System Git/curl implementation of the transport policies.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemTransport;
impl Transport for SystemTransport {
    fn object_type(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["cat-file", "-t", object], custom_ssh)
    }
    fn require_object(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["cat-file", "-e", object], custom_ssh).map(|_| ())
    }
    fn read_blob(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<Vec<u8>> {
        system::git_raw(repo, &["cat-file", "blob", object], custom_ssh)
    }
    fn list_tree(
        &self,
        repo: &Path,
        tree: &str,
        include_trees: bool,
        custom_ssh: bool,
    ) -> Result<Vec<u8>> {
        let mut args = vec!["ls-tree", "-r"];
        if include_trees {
            args.push("-t");
        }
        args.extend(["-z", tree]);
        system::git_raw(repo, &args, custom_ssh)
    }
    fn verify_revision(
        &self,
        repo: &Path,
        revision: &str,
        quiet: bool,
        custom_ssh: bool,
    ) -> Result<String> {
        let mut args = vec!["rev-parse", "--verify"];
        if quiet {
            args.push("-q");
        }
        args.push(revision);
        system::git(repo, &args, custom_ssh)
    }
    fn require_ancestor(
        &self,
        repo: &Path,
        ancestor: &str,
        descendant: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git(
            repo,
            &["merge-base", "--is-ancestor", ancestor, descendant],
            custom_ssh,
        )
        .map(|_| ())
    }
    fn checkout_detached(
        &self,
        repo: &Path,
        revision: &str,
        quiet: bool,
        custom_ssh: bool,
    ) -> Result<()> {
        let mut args = vec!["checkout"];
        if quiet {
            args.push("-q");
        }
        args.extend(["--detach", revision]);
        system::git(repo, &args, custom_ssh).map(|_| ())
    }
    fn refs_with_objects(&self, repo: &Path, prefix: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["for-each-ref", "--format=%(refname) %(objectname)", prefix],
            custom_ssh,
        )
    }
    fn delete_ref(&self, repo: &Path, name: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["update-ref", "-d", name], custom_ssh).map(|_| ())
    }
    fn update_ref(&self, repo: &Path, name: &str, object: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["update-ref", name, object], custom_ssh).map(|_| ())
    }
    fn tags_at(&self, repo: &Path, revision: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["tag", "--points-at", revision], custom_ssh)
    }
    fn config_get(&self, repo: &Path, key: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["config", "--get", key], custom_ssh)
    }
    fn config_blob(&self, repo: &Path, object: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["config", "--blob", object, "--null", "--list"],
            custom_ssh,
        )
    }
    fn config_file_add(
        &self,
        repo: &Path,
        file: &str,
        key: &str,
        value: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git(
            repo,
            &["config", "--file", file, "--add", key, value],
            custom_ssh,
        )
        .map(|_| ())
    }
    fn config_set(&self, repo: &Path, key: &str, value: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["config", key, value], custom_ssh).map(|_| ())
    }
    fn rename_remote(&self, repo: &Path, old: &str, new: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["remote", "rename", old, new], custom_ssh).map(|_| ())
    }
    fn branches_matching(&self, repo: &Path, pattern: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["branch", "--list", pattern], custom_ssh)
    }
    fn create_branch(
        &self,
        repo: &Path,
        branch: &str,
        start: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git(repo, &["checkout", "-q", "-b", branch, start], custom_ssh).map(|_| ())
    }
    fn remove_cached_path(&self, repo: &Path, path: &str, custom_ssh: bool) -> Result<()> {
        system::git(
            repo,
            &["rm", "-r", "-q", "--cached", "--ignore-unmatch", "--", path],
            custom_ssh,
        )
        .map(|_| ())
    }
    fn checkout_path(
        &self,
        repo: &Path,
        revision: &str,
        path: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git(repo, &["checkout", revision, "--", path], custom_ssh).map(|_| ())
    }
    fn stage_all(&self, repo: &Path, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["add", "-A"], custom_ssh).map(|_| ())
    }
    fn commit_without_hooks(&self, repo: &Path, message: &str, custom_ssh: bool) -> Result<()> {
        system::git(
            repo,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-q",
                "-m",
                message,
            ],
            custom_ssh,
        )
        .map(|_| ())
    }
    fn object_id(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["rev-parse", reference], custom_ssh)
    }
    fn resolve_commit(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
            custom_ssh,
        )
    }
    fn verified_object_id(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["rev-parse", "--verify", "-q", reference],
            custom_ssh,
        )
    }
    fn origin_default_branch(&self, repo: &Path, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
            custom_ssh,
        )
    }
    fn verify_ref(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<()> {
        system::git(
            repo,
            &["show-ref", "--verify", "--quiet", reference],
            custom_ssh,
        )
        .map(drop)
    }
    fn show_file(&self, repo: &Path, commit: &str, path: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["show", &format!("{commit}:{path}")], custom_ssh)
    }
    fn tree_paths(
        &self,
        repo: &Path,
        commit: &str,
        path: &str,
        custom_ssh: bool,
    ) -> Result<String> {
        system::git(
            repo,
            &["ls-tree", "--name-only", commit, "--", path],
            custom_ssh,
        )
    }
    fn tree_entries(&self, repo: &Path, commit: &str, custom_ssh: bool) -> Result<Vec<u8>> {
        system::git_raw(
            repo,
            &["ls-tree", "-r", "-t", "-z", "--full-tree", commit, "--"],
            custom_ssh,
        )
    }
    fn tags(&self, repo: &Path, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["tag", "--list"], custom_ssh)
    }
    fn status(
        &self,
        repo: &Path,
        ignored: bool,
        paths: &[&str],
        custom_ssh: bool,
    ) -> Result<Vec<u8>> {
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
        system::git_raw(repo, &args, custom_ssh)
    }
    fn refs(&self, repo: &Path, prefix: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["for-each-ref", "--format=%(refname)", prefix],
            custom_ssh,
        )
    }
    fn unpublished_count(&self, repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
        system::git(
            repo,
            &["rev-list", "--count", reference, "--not", "--remotes"],
            custom_ssh,
        )
    }
    fn stashes(&self, repo: &Path, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["stash", "list"], custom_ssh)
    }
    fn head_history(
        &self,
        repo: &Path,
        selection: HeadHistory<'_>,
        custom_ssh: bool,
    ) -> Result<String> {
        // Bound the displayed history while keeping reachability checks exact.
        let mut args = vec![
            "log",
            "--max-count=50",
            "--format=%H%x09%s",
            "HEAD",
            "--not",
        ];
        match selection {
            HeadHistory::Unpublished { exclude_tags } => {
                args.push("--remotes");
                if exclude_tags {
                    args.push("--tags");
                }
            }
            HeadHistory::Unreferenced { keep } => {
                args.extend(["--branches", "--tags", "--remotes"]);
                args.extend(keep.iter().copied());
            }
        }
        system::git(repo, &args, custom_ssh)
    }
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
        system::git(repo, args, custom_ssh)
    }
    fn git_with_input(
        &self,
        repo: &Path,
        args: &[&str],
        input: Vec<u8>,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git_with_input(repo, args, input, custom_ssh)
    }
    fn ls_remote(&self, source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote> {
        system::ls_remote(source, custom_ssh, start_error)
    }
    fn ls_remote_refs(&self, source: &str, custom_ssh: bool) -> Result<RemoteRefs> {
        system::ls_remote_refs(source, custom_ssh)
    }
    fn http_get(&self, url: &str) -> Result<Option<Vec<u8>>> {
        system::http_get(url)
    }
    fn clone_repo(
        &self,
        parent: &Path,
        source: &str,
        destination: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        system::git(
            parent,
            &["clone", "--no-local", "--", source, destination],
            custom_ssh,
        )?;
        Ok(())
    }
    fn fetch(&self, repo: &Path, request: Fetch<'_>, custom_ssh: bool) -> Result<()> {
        match request {
            Fetch::Revision { remote, revision } => {
                system::git(repo, &["fetch", remote, revision], custom_ssh)?;
            }
            Fetch::Tags { remote, prune } => {
                let mut args = vec!["fetch", remote];
                if prune {
                    args.extend(["--prune", "--prune-tags"]);
                }
                args.extend(["--force", "--tags"]);
                system::git(repo, &args, custom_ssh)?;
            }
            Fetch::RecordTags { remote, namespace } => {
                system::git(
                    repo,
                    &[
                        "fetch",
                        "--quiet",
                        "--prune",
                        "--no-tags",
                        remote,
                        &format!("+refs/tags/*:{namespace}*"),
                    ],
                    custom_ssh,
                )?;
            }
        }
        Ok(())
    }
    fn set_remote_head(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<()> {
        system::git(repo, &["remote", "set-head", remote, "--auto"], custom_ssh)?;
        Ok(())
    }
    fn remote_tags(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<String> {
        system::git(repo, &["ls-remote", "--tags", remote], custom_ssh)
    }
}

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod local;
#[cfg(test)]
mod network;

pub(crate) fn git(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
    SystemTransport.git(repo, args, custom_ssh)
}
pub(crate) fn git_with_input(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    custom_ssh: bool,
) -> Result<()> {
    SystemTransport.git_with_input(repo, args, input, custom_ssh)
}
pub(crate) fn ls_remote(source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote> {
    SystemTransport.ls_remote(source, custom_ssh, start_error)
}
pub(crate) fn ls_remote_refs(source: &str, custom_ssh: bool) -> Result<RemoteRefs> {
    SystemTransport.ls_remote_refs(source, custom_ssh)
}
pub(crate) fn http_get(url: &str) -> Result<Option<Vec<u8>>> {
    SystemTransport.http_get(url)
}
pub(crate) fn clone_repo(
    parent: &Path,
    source: &str,
    destination: &str,
    custom_ssh: bool,
) -> Result<()> {
    SystemTransport.clone_repo(parent, source, destination, custom_ssh)
}
pub(crate) fn fetch(repo: &Path, request: Fetch<'_>, custom_ssh: bool) -> Result<()> {
    SystemTransport.fetch(repo, request, custom_ssh)
}
pub(crate) fn remote_tags(repo: &Path, remote: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.remote_tags(repo, remote, custom_ssh)
}

pub(crate) fn set_remote_head(repo: &Path, remote: &str, custom_ssh: bool) -> Result<()> {
    SystemTransport.set_remote_head(repo, remote, custom_ssh)
}

pub(crate) fn object_id(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.object_id(repo, reference, custom_ssh)
}

pub(crate) fn resolve_commit(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.resolve_commit(repo, reference, custom_ssh)
}

pub(crate) fn verified_object_id(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.verified_object_id(repo, reference, custom_ssh)
}

pub(crate) fn origin_default_branch(repo: &Path, custom_ssh: bool) -> Result<String> {
    SystemTransport.origin_default_branch(repo, custom_ssh)
}

pub(crate) fn verify_ref(repo: &Path, reference: &str, custom_ssh: bool) -> Result<()> {
    SystemTransport.verify_ref(repo, reference, custom_ssh)
}

pub(crate) fn show_file(repo: &Path, commit: &str, path: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.show_file(repo, commit, path, custom_ssh)
}

pub(crate) fn tree_paths(
    repo: &Path,
    commit: &str,
    path: &str,
    custom_ssh: bool,
) -> Result<String> {
    SystemTransport.tree_paths(repo, commit, path, custom_ssh)
}

pub(crate) fn tree_entries(repo: &Path, commit: &str, custom_ssh: bool) -> Result<Vec<u8>> {
    SystemTransport.tree_entries(repo, commit, custom_ssh)
}

pub(crate) fn tags(repo: &Path, custom_ssh: bool) -> Result<String> {
    SystemTransport.tags(repo, custom_ssh)
}

pub(crate) fn checkout_detached(repo: &Path, commit: &str, custom_ssh: bool) -> Result<()> {
    SystemTransport.checkout_detached(repo, commit, false, custom_ssh)
}

pub(crate) fn status(
    repo: &Path,
    ignored: bool,
    paths: &[&str],
    custom_ssh: bool,
) -> Result<Vec<u8>> {
    SystemTransport.status(repo, ignored, paths, custom_ssh)
}

pub(crate) fn refs(repo: &Path, prefix: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.refs(repo, prefix, custom_ssh)
}

pub(crate) fn unpublished_count(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport.unpublished_count(repo, reference, custom_ssh)
}

pub(crate) fn stashes(repo: &Path, custom_ssh: bool) -> Result<String> {
    SystemTransport.stashes(repo, custom_ssh)
}

pub(crate) fn head_history(
    repo: &Path,
    selection: HeadHistory<'_>,
    custom_ssh: bool,
) -> Result<String> {
    SystemTransport.head_history(repo, selection, custom_ssh)
}
