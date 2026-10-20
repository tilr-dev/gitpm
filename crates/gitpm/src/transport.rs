//! Convert transport failures to gitpm's stable public errors.
use crate::Result;
use gitpm_transport::LsRemote;
pub(crate) use gitpm_transport::{Fetch, RemoteRefs};
use gitpm_transport::{SystemTransport, Transport};
use std::path::Path;
pub(crate) fn git(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
    SystemTransport
        .git(repo, args, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn git_raw(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>> {
    SystemTransport
        .git_raw(repo, args, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn git_with_input(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    custom_ssh: bool,
) -> Result<()> {
    SystemTransport
        .git_with_input(repo, args, input, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn ls_remote(source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote> {
    SystemTransport
        .ls_remote(source, custom_ssh, start_error)
        .map_err(Into::into)
}
pub(crate) fn ls_remote_refs(source: &str, custom_ssh: bool) -> Result<RemoteRefs> {
    SystemTransport
        .ls_remote_refs(source, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn http_get(url: &str) -> Result<Option<Vec<u8>>> {
    SystemTransport.http_get(url).map_err(Into::into)
}
pub(crate) fn clone_repo(
    parent: &Path,
    source: &str,
    destination: &str,
    custom_ssh: bool,
) -> Result<()> {
    SystemTransport
        .clone_repo(parent, source, destination, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn fetch(repo: &Path, request: Fetch<'_>, custom_ssh: bool) -> Result<()> {
    SystemTransport
        .fetch(repo, request, custom_ssh)
        .map_err(Into::into)
}
pub(crate) fn remote_tags(repo: &Path, remote: &str, custom_ssh: bool) -> Result<String> {
    SystemTransport
        .remote_tags(repo, remote, custom_ssh)
        .map_err(Into::into)
}
