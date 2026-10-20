//! Replaceable, bounded Git and HTTP transport.
mod error;
#[cfg(test)]
mod test_support;
mod transport;
pub use error::{Error, Result};
use std::path::Path;
pub use transport::{LsRemote, RemoteRefs};

/// Fetch operations without command-line flags.
#[derive(Clone, Copy, Debug)]
pub enum Fetch<'a> {
    /// Force-update tags, optionally pruning deleted origin tags.
    Tags { remote: &'a str, prune: bool },
    /// Mirror remote tags into a private local ref namespace.
    RecordTags { remote: &'a str, namespace: &'a str },
}
/// External Git/HTTP operations. Raw local Git remains for staged migration.
pub trait Transport {
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String>;
    fn git_raw(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>>;
    fn git_with_input(
        &self,
        repo: &Path,
        args: &[&str],
        input: Vec<u8>,
        custom_ssh: bool,
    ) -> Result<()>;
    fn git_config_defined(&self, key: &str, start_error: &str) -> Result<bool>;
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
    fn remote_tags(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<String>;
}
/// System Git/curl implementation of the transport policies.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemTransport;
impl Transport for SystemTransport {
    fn git(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
        transport::git(repo, args, custom_ssh)
    }
    fn git_raw(&self, repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>> {
        transport::git_raw(repo, args, custom_ssh)
    }
    fn git_with_input(
        &self,
        repo: &Path,
        args: &[&str],
        input: Vec<u8>,
        custom_ssh: bool,
    ) -> Result<()> {
        transport::git_with_input(repo, args, input, custom_ssh)
    }
    fn git_config_defined(&self, key: &str, start_error: &str) -> Result<bool> {
        transport::git_config_defined(key, start_error)
    }
    fn ls_remote(&self, source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote> {
        transport::ls_remote(source, custom_ssh, start_error)
    }
    fn ls_remote_refs(&self, source: &str, custom_ssh: bool) -> Result<RemoteRefs> {
        transport::ls_remote_refs(source, custom_ssh)
    }
    fn http_get(&self, url: &str) -> Result<Option<Vec<u8>>> {
        transport::http_get(url)
    }
    fn clone_repo(
        &self,
        parent: &Path,
        source: &str,
        destination: &str,
        custom_ssh: bool,
    ) -> Result<()> {
        transport::git(
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
                transport::git(repo, &args, custom_ssh)?;
            }
            Fetch::RecordTags { remote, namespace } => {
                transport::git(
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
    fn remote_tags(&self, repo: &Path, remote: &str, custom_ssh: bool) -> Result<String> {
        transport::git(repo, &["ls-remote", "--tags", remote], custom_ssh)
    }
}

#[cfg(test)]
mod message_tests;
