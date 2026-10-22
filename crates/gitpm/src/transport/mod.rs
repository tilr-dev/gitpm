//! Replaceable, bounded Git and HTTP transport.
mod system;
use crate::error::Result;
use std::path::Path;
pub(crate) use system::{LsRemote, RemoteRefs};

/// Fetch operations without command-line flags.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Fetch<'a> {
    /// Force-update tags, optionally pruning deleted origin tags.
    Tags { remote: &'a str, prune: bool },
    /// Mirror remote tags into a private local ref namespace.
    RecordTags { remote: &'a str, namespace: &'a str },
}
/// External Git/HTTP operations. Raw local Git remains for staged migration.
pub(crate) trait Transport {
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String>;
    fn git_raw(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>>;
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
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
        system::git(repo, args, custom_ssh)
    }
    fn git_raw(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>> {
        system::git_raw(repo, args, custom_ssh)
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
mod network;

pub(crate) fn git(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
    SystemTransport.git(repo, args, custom_ssh)
}
pub(crate) fn git_raw(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>> {
    SystemTransport.git_raw(repo, args, custom_ssh)
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
