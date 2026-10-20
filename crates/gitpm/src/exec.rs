//! The only place that starts subprocesses. Git and curl are system tools for now;
//! this boundary is where gix/ureq can replace them later.
use crate::error::{Context, Error, Result, ensure};
use std::{
    collections::BTreeMap,
    io::Read,
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
const BATCH_SSH: &str = "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o StrictHostKeyChecking=accept-new";

/// `git -C repo` that can never wait for user input: no terminal prompts, closed
/// stdin, bounded transfers. The user's own `core.sshCommand` or `GIT_SSH_COMMAND`
/// (inherited by the child) keeps precedence over the batch SSH command.
fn git_command(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Command> {
    let mut command = git_base(custom_ssh);
    command.arg("-C").arg(repo).args(args);
    Ok(command)
}

/// The bounded, noninteractive `git` of [`git_command`] without a repository.
fn git_base(custom_ssh: bool) -> Command {
    static CONFIGURED_SSH: OnceLock<bool> = OnceLock::new();
    let custom_ssh = custom_ssh
        || *CONFIGURED_SSH.get_or_init(|| {
            git_config_defined("core.sshCommand", "cannot start git").unwrap_or(true)
        });
    git_base_with_policy(custom_ssh)
}

fn git_base_with_policy(custom_ssh: bool) -> Command {
    let mut command = noninteractive_git();
    command.args(["-c", "http.lowSpeedLimit=1", "-c", "http.lowSpeedTime=20"]);
    if !custom_ssh {
        command
            .arg("-c")
            .arg(format!("core.sshCommand={BATCH_SSH}"));
    }
    command
}

/// Apply the input policy before adding arguments specific to a Git invocation.
fn noninteractive_git() -> Command {
    let mut command = Command::new("git");
    command
        .args(["-c", "credential.interactive=never"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .stdin(Stdio::null());
    command
}

/// Run `write` on pipe `fd` so a closed reader yields EPIPE rather than SIGPIPE.
#[cfg(target_os = "macos")]
fn without_sigpipe<T>(fd: std::os::fd::RawFd, write: impl FnOnce() -> T) -> T {
    // macOS sends SIGPIPE to the process, so a thread mask does not help; the
    // descriptor flag suppresses it. `F_SETNOSIGPIPE` from <sys/fcntl.h>.
    const F_SETNOSIGPIPE: libc::c_int = 73;
    // SAFETY: fcntl on a pipe this thread owns.
    unsafe { libc::fcntl(fd, F_SETNOSIGPIPE, 1) };
    write()
}

/// Run `write` on pipe `fd` so a closed reader yields EPIPE rather than SIGPIPE.
#[cfg(not(target_os = "macos"))]
fn without_sigpipe<T>(_fd: std::os::fd::RawFd, write: impl FnOnce() -> T) -> T {
    // Linux sends SIGPIPE of a write to the writing thread: block it there and
    // take a pending one before unblocking, so it is never delivered.
    // SAFETY: sigset operations on local values; changes the mask of this thread only.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
        let result = write();
        let mut pending: libc::sigset_t = std::mem::zeroed();
        libc::sigpending(&mut pending);
        if libc::sigismember(&pending, libc::SIGPIPE) == 1 {
            let mut taken = 0;
            libc::sigwait(&set, &mut taken);
        }
        libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        result
    }
}

/// Run to completion with a wall-clock limit; stdout/stderr are drained concurrently.
fn output_within(command: Command, limit: Duration, what: &str) -> Result<Output> {
    output_with_input_within(command, None, limit, what)
}

/// Supplied input is written concurrently so a child that stops reading is still bounded.
fn output_with_input_within(
    mut command: Command,
    input: Option<Vec<u8>>,
    limit: Duration,
    what: &str,
) -> Result<Output> {
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
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
    let writer = input.map(|input| {
        let mut stdin = child.stdin.take().expect("piped stdin");
        std::thread::spawn(move || {
            use std::io::Write;
            // A child that exits before reading must not kill a host (FFI) that keeps
            // the default SIGPIPE action: the write fails with EPIPE instead.
            let fd = std::os::fd::AsRawFd::as_raw_fd(&stdin);
            without_sigpipe(fd, || stdin.write_all(&input))
        })
    });
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
    if let Some(writer) = writer {
        let written = writer
            .join()
            .map_err(|_| Error::Git("git input writer panicked".into()))?;
        if status.success() {
            written.context(Error::Git, "cannot write git input")?;
        }
    }
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// `git -C repo ARGS`, noninteractive. Returns trimmed stdout.
pub(crate) fn git(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<String> {
    let output = git_raw(repo, args, custom_ssh)?;
    Ok(String::from_utf8(output)?.trim().to_owned())
}

/// Raw stdout (untrimmed) for NUL-separated output such as `status -z`.
pub(crate) fn git_raw(repo: &Path, args: &[&str], custom_ssh: bool) -> Result<Vec<u8>> {
    let what = format!("git {}", args.join(" "));
    let output = output_within(git_command(repo, args, custom_ssh)?, GIT_TIMEOUT, &what)?;
    ensure!(
        output.status.success(),
        Git,
        "{what}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

/// Run Git with library-owned input, never inherited user stdin. Input closes at EOF.
pub(crate) fn git_with_input(repo: &Path, args: &[&str], input: Vec<u8>) -> Result<()> {
    let what = format!("git {}", args.join(" "));
    let output =
        output_with_input_within(git_command(repo, args)?, Some(input), GIT_TIMEOUT, &what)?;
    ensure!(
        output.status.success(),
        Git,
        "{what}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Tags and branches of a remote, as `git ls-remote` lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RemoteRefs {
    /// Tag name (without `refs/tags/`, peeled `^{}` entries folded into their tag) →
    /// the commit it points to.
    pub(crate) tags: BTreeMap<String, String>,
    /// Branch name (without `refs/heads/`) → its HEAD commit.
    pub(crate) heads: BTreeMap<String, String>,
}

/// Tags and branches of a remote: `git ls-remote --tags --heads -- SOURCE` with the
/// bounds of every repository command (no prompts, batch SSH, low-speed limit,
/// wall-clock kill). Nothing is fetched or written. Failure is [`Error::Git`] with git's
/// stderr (`LC_ALL=C`, so it can be classified). Runs in the OS temp dir with only the
/// file, git, http(s) and ssh transports; callers validate the source first
/// (`auth::validate_recorded`): a URL or an absolute path, no remote helper.
pub(crate) fn ls_remote_refs(source: &str, custom_ssh: bool) -> Result<RemoteRefs> {
    let what = format!("git ls-remote --tags --heads -- {source}");
    let mut command = git_base(custom_ssh);
    // Outside any repository: a bare word is a path, never a remote name of the cwd.
    // An explicit transport allowlist: a source from a foreign manifest never reaches
    // `ext::`, `fd::` or any other remote helper. (`GIT_PROTOCOL_FROM_USER=0` alone would
    // also block local paths: since Git 2.38.1 `protocol.file.allow` defaults to `user`.)
    command.args(["-c", "protocol.allow=never"]);
    for transport in ["file", "git", "http", "https", "ssh"] {
        command
            .arg("-c")
            .arg(format!("protocol.{transport}.allow=always"));
    }
    command
        .args(["ls-remote", "--tags", "--heads", "--", source])
        .env("LC_ALL", "C")
        .current_dir(std::env::temp_dir());
    let output = output_within(command, GIT_TIMEOUT, &what)?;
    ensure!(
        output.status.success(),
        Git,
        "{what}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut refs = RemoteRefs::default();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((commit, name)) = line.split_once('\t') else {
            continue;
        };
        if let Some(branch) = name.strip_prefix("refs/heads/") {
            refs.heads.insert(branch.to_owned(), commit.to_owned());
            continue;
        }
        let Some(name) = name.strip_prefix("refs/tags/") else {
            continue;
        };
        match name.strip_suffix("^{}") {
            // The peeled commit of an annotated tag wins over the tag object.
            Some(name) => {
                refs.tags.insert(name.to_owned(), commit.to_owned());
            }
            None => {
                refs.tags
                    .entry(name.to_owned())
                    .or_insert_with(|| commit.to_owned());
            }
        }
    }
    Ok(refs)
}

/// Whether the user's Git configuration (outside any repository) defines `key`.
pub(crate) fn git_config_defined(key: &str, start_error: &str) -> Result<bool> {
    Ok(git_config_command(key)
        .output()
        .context(Error::Auth, start_error)?
        .status
        .success())
}

fn git_config_command(key: &str) -> Command {
    let mut command = noninteractive_git();
    command.args(["config", "--get", key]);
    command
}

/// Outcome of a read-only `git ls-remote`.
pub(crate) struct LsRemote {
    pub success: bool,
    pub stderr: String,
}

/// `git ls-remote -- SOURCE` without cloning; stdout is discarded.
pub(crate) fn ls_remote(source: &str, custom_ssh: bool, start_error: &str) -> Result<LsRemote> {
    let output = ls_remote_command(source, custom_ssh)
        .output()
        .context(Error::Auth, start_error)?;
    Ok(LsRemote {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn ls_remote_command(source: &str, custom_ssh: bool) -> Command {
    let mut command = git_base(custom_ssh);
    command
        .args(["ls-remote", "--", source])
        .env("LC_ALL", "C")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

/// HTTP GET result. `None` body means HTTP 404. Redirects are not followed (a 3xx
/// is an error, so a catalog cannot be moved to another host silently) and bodies
/// over 16 MiB are refused.
pub(crate) fn http_get(url: &str) -> Result<Option<Vec<u8>>> {
    let mut command = Command::new("curl");
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "5",
        "--max-time",
        "20",
        "--proto",
        "=https,http",
        "--max-filesize",
        "16777216",
        "--write-out",
        "\n%{http_code}",
        "--url",
        url,
    ]);
    http_response(command)
}

const HTTP_BODY_LIMIT: usize = 16 * 1024 * 1024;
const HTTP_STATUS_LEN: usize = 4; // newline plus three status digits

fn http_response(mut command: Command) -> Result<Option<Vec<u8>>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(Error::Http, "cannot start curl")?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    // One extra byte proves overflow; never buffer the rest of the stream.
    let mut stdout = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped stdout")
        .take((HTTP_BODY_LIMIT + HTTP_STATUS_LEN + 1) as u64)
        .read_to_end(&mut stdout);
    let oversized = stdout.len() > HTTP_BODY_LIMIT + HTTP_STATUS_LEN;
    if oversized || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    let stderr = errors.join().unwrap_or_default();
    read.context(Error::Http, "cannot read catalog response")?;
    ensure!(!oversized, Http, "catalog response exceeds 16 MiB");
    let status = status.context(Error::Http, "cannot wait for curl")?;
    ensure!(
        status.success(),
        Http,
        "catalog request failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    let split = stdout
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or_else(|| Error::Http("missing HTTP status".into()))?;
    ensure!(
        split <= HTTP_BODY_LIMIT,
        Http,
        "catalog response exceeds 16 MiB"
    );
    let status = &stdout[split + 1..];
    if status == b"404" {
        return Ok(None);
    }
    ensure!(
        status == b"200",
        Http,
        "catalog HTTP {}",
        String::from_utf8_lossy(status)
    );
    stdout.truncate(split);
    Ok(Some(stdout))
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
        let head = git(&author.path, &["rev-parse", "HEAD"], false).unwrap();
        assert_eq!(head.len(), 40);
        assert!(!head.ends_with('\n'));
        let error = git(&author.path, &["rev-parse", "--verify", "nope"], false).unwrap_err();
        assert_eq!(error.kind(), "git");
        let message = error.to_string();
        assert!(
            message.starts_with("git rev-parse --verify nope: "),
            "{message}"
        );
        // A missing repository directory is a git failure too, not a panic.
        let error = git(&dir.join("missing"), &["status"], false).unwrap_err();
        assert_eq!(error.kind(), "git");
    }

    #[test]
    fn git_raw_keeps_nul_separated_output_untrimmed() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        fs::write(author.path.join("a b.txt"), "x").unwrap();
        let raw = git_raw(&author.path, &["status", "--porcelain=v1", "-z"], false).unwrap();
        assert_eq!(raw, b"?? a b.txt\0");
        assert_eq!(
            git_raw(&author.path, &["bad-command"], false)
                .unwrap_err()
                .kind(),
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
    fn input_to_a_child_that_closed_stdin_fails_without_sigpipe() {
        // A host process (FFI) keeps the default SIGPIPE action, unlike a Rust `main`.
        let previous = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
        let mut command = Command::new("sh");
        command.args(["-c", "exec 0<&-; exit 0"]);
        // More than a pipe buffer: the write must hit the closed pipe.
        let result = output_with_input_within(
            command,
            Some(vec![b'x'; 1 << 20]),
            Duration::from_secs(60),
            "sh",
        );
        unsafe { libc::signal(libc::SIGPIPE, previous) };
        let error = result.unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(
            error.to_string().contains("cannot write git input"),
            "{error}"
        );
    }

    #[test]
    fn git_never_reads_stdin_or_prompts() {
        let command = git_base_with_policy(true);
        assert!(
            !command
                .get_args()
                .any(|arg| arg.to_string_lossy().starts_with("core.sshCommand="))
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "GIT_TERMINAL_PROMPT"
                    && value == Some(std::ffi::OsStr::new("0")))
        );
        assert!(!command.get_envs().any(|(key, _)| key == "GIT_SSH_COMMAND"));
        let command = git_base_with_policy(false);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        // Deterministically exercise both policies independently of the host config.
        assert!(args.contains(&format!("core.sshCommand={BATCH_SSH}")));
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "demo1");
        let commands = [
            git_command(&author.path, &["status"], false).unwrap(),
            git_config_command("core.sshCommand"),
            ls_remote_command("https://example.invalid/repo.git", false),
        ];
        for command in &commands {
            let args: Vec<_> = command.get_args().collect();
            assert!(
                args.windows(2).any(|pair| {
                    pair[0] == std::ffi::OsStr::new("-c")
                        && pair[1] == std::ffi::OsStr::new("credential.interactive=never")
                }),
                "missing noninteractive credential config: {command:?}"
            );
            for (name, expected) in [("GIT_TERMINAL_PROMPT", "0"), ("GIT_ASKPASS", "")] {
                assert_eq!(
                    command
                        .get_envs()
                        .find(|(key, _)| *key == std::ffi::OsStr::new(name))
                        .map(|(_, value)| value),
                    Some(Some(std::ffi::OsStr::new(expected))),
                    "missing {name}: {command:?}"
                );
            }
        }

        // Git reads EOF from the explicitly closed stdin without waiting.
        let output = git_command(&author.path, &["hash-object", "--stdin"], false)
            .unwrap()
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
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
        let ok = ls_remote(author.source(), false, "start").unwrap();
        assert!(ok.success);
        let missing = dir.join("missing");
        let failed = ls_remote(missing.to_str().unwrap(), false, "start").unwrap();
        assert!(!failed.success);
        assert!(!failed.stderr.is_empty());
    }

    #[test]
    fn ls_remote_refs_lists_tags_with_peeled_commits_and_branches_and_fails_as_git() {
        let dir = TempDir::new("exec");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        let first = author.head();
        let second = author.commit("0.2.0");
        crate::test_support::git(
            &author.path,
            &["tag", "-a", "-m", "release", "dan/demo1@0.2.0"],
        );
        let refs = ls_remote_refs(author.source(), false).unwrap();
        assert_eq!(
            refs.heads,
            BTreeMap::from([("main".to_owned(), second.clone())])
        );
        assert_eq!(
            refs.tags,
            BTreeMap::from([
                ("dan/demo1@0.1.0".to_owned(), first),
                ("dan/demo1@0.2.0".to_owned(), second),
            ])
        );
        // Remote helpers are refused by git itself, even if a caller forgot to validate.
        let marker = dir.join("pwned");
        let error =
            ls_remote_refs(&format!("ext::sh -c touch% {}", marker.display()), false).unwrap_err();
        assert!(error.to_string().contains("not allowed"), "{error}");
        assert!(!marker.exists());
        let error = ls_remote_refs(dir.join("missing").to_str().unwrap(), false).unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(
            error
                .to_string()
                .starts_with("git ls-remote --tags --heads -- "),
            "{error}"
        );
    }

    fn streamed_response(size: usize, status: u16) -> Result<Option<Vec<u8>>> {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/catalog.json", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Read the complete request before writing, without timing assumptions.
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let chunk = [b' '; 8192];
            let mut send = || -> std::io::Result<()> {
                for start in (0..size).step_by(chunk.len()) {
                    let len = (size - start).min(chunk.len());
                    write!(stream, "{len:x}\r\n")?;
                    stream.write_all(&chunk[..len])?;
                    stream.write_all(b"\r\n")?;
                }
                stream.write_all(b"0\r\n\r\n")
            };
            let _ = send(); // gitpm closes the connection on overflow.
        });
        // Emulate curl versions that do not enforce --max-filesize on streams.
        let mut command = Command::new("curl");
        command.args([
            "--disable",
            "--silent",
            "--max-time",
            "20",
            "--write-out",
            "\n%{http_code}",
            "--url",
            &url,
        ]);
        let result = http_response(command);
        server.join().unwrap();
        result
    }

    #[test]
    fn oversized_streamed_catalog_response_is_refused() {
        let error = streamed_response(50 * 1024 * 1024, 200).unwrap_err();
        assert_eq!(error.kind(), "http");
        assert!(error.to_string().contains("exceeds 16 MiB"), "{error}");
    }

    #[test]
    fn streamed_catalog_response_size_boundary_and_status() {
        for size in [0, 17, HTTP_BODY_LIMIT] {
            assert_eq!(
                streamed_response(size, 200).unwrap().unwrap(),
                vec![b' '; size]
            );
        }
        for extra in 1..=5 {
            assert_eq!(
                streamed_response(HTTP_BODY_LIMIT + extra, 200)
                    .unwrap_err()
                    .kind(),
                "http"
            );
        }
        assert_eq!(streamed_response(17, 404).unwrap(), None);
        assert_eq!(streamed_response(0, 301).unwrap_err().kind(), "http");
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
