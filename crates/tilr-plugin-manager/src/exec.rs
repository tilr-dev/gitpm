//! The only place that starts subprocesses. Git and curl are system tools for now;
//! this boundary is where gix/ureq can replace them later.
use crate::error::{Context, Error, Result, ensure};
use std::{
    path::Path,
    process::{Command, Stdio},
};

/// `git -C repo ARGS`, noninteractive. Returns trimmed stdout.
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context(Error::Git, "cannot start git")?;
    ensure!(
        output.status.success(),
        Git,
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// Raw stdout (untrimmed) for NUL-separated output such as `status -z`.
pub(crate) fn git_raw(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context(Error::Git, "cannot start git")?;
    ensure!(
        output.status.success(),
        Git,
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

/// Whether the user's Git configuration (outside any repository) defines `key`.
pub(crate) fn git_config_defined(key: &str, start_error: &str) -> Result<bool> {
    Ok(Command::new("git")
        .args(["config", "--get", key])
        .output()
        .context(Error::Auth, start_error)?
        .status
        .success())
}

/// Outcome of a read-only `git ls-remote`.
pub(crate) struct LsRemote {
    pub success: bool,
    pub stderr: String,
}

/// `git ls-remote -- SOURCE` without cloning; stdout is discarded.
pub(crate) fn ls_remote(
    source: &str,
    ssh_command: Option<&str>,
    start_error: &str,
) -> Result<LsRemote> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "http.lowSpeedLimit=1",
            "-c",
            "http.lowSpeedTime=20",
            "ls-remote",
            "--",
            source,
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(ssh) = ssh_command {
        command.env("GIT_SSH_COMMAND", ssh);
    }
    let output = command.output().context(Error::Auth, start_error)?;
    Ok(LsRemote {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// HTTP GET result. `None` body means HTTP 404.
pub(crate) fn http_get(url: &str) -> Result<Option<Vec<u8>>> {
    let output = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "5",
            "--max-time",
            "20",
            "--write-out",
            "\n%{http_code}",
            "--url",
            url,
        ])
        .output()
        .context(Error::Http, "cannot start curl")?;
    ensure!(
        output.status.success(),
        Http,
        "catalog request failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let split = output
        .stdout
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or_else(|| Error::Http("missing HTTP status".into()))?;
    let status = &output.stdout[split + 1..];
    if status == b"404" {
        return Ok(None);
    }
    ensure!(
        status == b"200",
        Http,
        "catalog HTTP {}",
        String::from_utf8_lossy(status)
    );
    Ok(Some(output.stdout[..split].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir};
    use std::fs;

    #[test]
    fn git_returns_trimmed_stdout_and_reports_failures_as_git_errors() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        let head = git(&author.path, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head.len(), 40);
        assert!(!head.ends_with('\n'));
        let error = git(&author.path, &["rev-parse", "--verify", "nope"]).unwrap_err();
        assert_eq!(error.kind(), "git");
        let message = error.to_string();
        assert!(
            message.starts_with("git rev-parse --verify nope: "),
            "{message}"
        );
        // A missing repository directory is a git failure too, not a panic.
        let error = git(&dir.join("missing"), &["status"]).unwrap_err();
        assert_eq!(error.kind(), "git");
    }

    #[test]
    fn git_raw_keeps_nul_separated_output_untrimmed() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        fs::write(author.path.join("a b.txt"), "x").unwrap();
        let raw = git_raw(&author.path, &["status", "--porcelain=v1", "-z"]).unwrap();
        assert_eq!(raw, b"?? a b.txt\0");
        assert_eq!(
            git_raw(&author.path, &["bad-command"]).unwrap_err().kind(),
            "git"
        );
    }

    #[test]
    fn git_config_defined_detects_missing_keys() {
        assert!(!git_config_defined("pm-unit-test.surely-undefined-key", "start").unwrap());
    }

    #[test]
    fn ls_remote_reports_success_and_stderr_without_failing() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        let ok = ls_remote(author.source(), Some("false"), "start").unwrap();
        assert!(ok.success);
        let missing = dir.join("missing");
        let failed = ls_remote(missing.to_str().unwrap(), None, "start").unwrap();
        assert!(!failed.success);
        assert!(!failed.stderr.is_empty());
    }

    #[test]
    fn http_get_rejects_non_http_responses() {
        // curl reports status 000 for file:// URLs; only HTTP 200 counts as a body.
        let dir = TempDir::new("exec");
        let file = dir.join("catalog.json");
        fs::write(&file, "{}").unwrap();
        let error = http_get(&format!("file://{}", file.display())).unwrap_err();
        assert_eq!(error.kind(), "http");
        let error = http_get(&format!("file://{}", dir.join("missing").display())).unwrap_err();
        assert_eq!(error.kind(), "http");
    }
}
