//! The only place that starts subprocesses. Git and curl are system tools for now;
//! this boundary is where gix/ureq can replace them later.
use crate::error::{Context, Error, Result, ensure};
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// Upper bound for one git subprocess; network stalls are cut earlier by
/// `http.lowSpeed*` and SSH keepalives, this only stops a wedged process.
const GIT_TIMEOUT: Duration = Duration::from_secs(600);
/// Noninteractive SSH for git transports: never prompts for passphrases or
/// unknown host keys, and gives up on dead connections.
const BATCH_SSH: &str = "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4";

/// `git -C repo` that can never wait for user input: no terminal prompts, closed
/// stdin, bounded transfers. The user's own `core.sshCommand` or `GIT_SSH_COMMAND`
/// (inherited by the child) keeps precedence over the batch SSH command.
fn git_command(repo: &Path, args: &[&str]) -> Result<Command> {
    static CUSTOM_SSH: OnceLock<bool> = OnceLock::new();
    let custom_ssh = match CUSTOM_SSH.get() {
        Some(value) => *value,
        None => *CUSTOM_SSH.get_or_init(|| {
            git_config_defined("core.sshCommand", "cannot start git").unwrap_or(true)
        }),
    };
    let mut command = Command::new("git");
    command.args([
        "-c",
        "http.lowSpeedLimit=1",
        "-c",
        "http.lowSpeedTime=20",
    ]);
    if !custom_ssh {
        command.arg("-c").arg(format!("core.sshCommand={BATCH_SSH}"));
    }
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    Ok(command)
}

/// Run to completion with a wall-clock limit; stdout/stderr are drained concurrently.
fn output_within(mut command: Command, limit: Duration, what: &str) -> Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(Error::Git, "cannot start git")?;
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut buffer);
            }
            buffer
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Git(format!(
                "{what}: timed out after {}s",
                limit.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// `git -C repo ARGS`, noninteractive. Returns trimmed stdout.
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = git_raw(repo, args)?;
    Ok(String::from_utf8(output)?.trim().to_owned())
}

/// Raw stdout (untrimmed) for NUL-separated output such as `status -z`.
pub(crate) fn git_raw(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let what = format!("git {}", args.join(" "));
    let output = output_within(git_command(repo, args)?, GIT_TIMEOUT, &what)?;
    ensure!(
        output.status.success(),
        Git,
        "{what}: {}",
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
    fn wedged_subprocess_is_killed_at_the_limit() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let started = Instant::now();
        let error = output_within(command, Duration::from_millis(200), "sleep").unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(error.to_string().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn git_never_reads_stdin_or_prompts() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        // A credential prompt would block; with prompts disabled it fails fast.
        let error = git(
            &author.path,
            &["ls-remote", "--", "https://127.0.0.1:9/none.git"],
        )
        .unwrap_err();
        assert_eq!(error.kind(), "git");
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
