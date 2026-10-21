//! Check repository read access using the user's existing Git credentials.
use crate::error::{Result, ensure, fail};
use crate::{Options, transport};
use serde::Serialize;
use std::path::Path;

/// Successful read-access check.
#[derive(Debug, Clone, Serialize)]
pub struct AuthCheck {
    pub source: String,
    /// Human-readable confirmation.
    pub message: String,
}

fn validate(source: &str) -> Result<()> {
    basic(source)?;
    if Path::new(source).exists() {
        return Ok(());
    }
    url_rules(source)
}

fn basic(source: &str) -> Result<()> {
    ensure!(
        !source.is_empty() && !source.starts_with('-') && !source.chars().any(char::is_control),
        InvalidArgument,
        "Specify a Git URL or a local path."
    );
    Ok(())
}

/// A recorded source that gitpm queries on its own (`packages.<id>.source` in `check`,
/// `forkOf.source`), possibly from a foreign manifest: an absolute path, or a Git URL
/// (`scheme://` of the supported schemes, or `host:path`) without remote helpers
/// (`::`) or credentials. Pure syntax: nothing is read from disk or run.
pub(crate) fn validate_recorded(source: &str) -> Result<()> {
    basic(source)?;
    ensure!(
        !source.contains("::"),
        InvalidArgument,
        "Git remote helpers are not supported."
    );
    if Path::new(source).is_absolute() {
        return Ok(());
    }
    let url = source.contains("://")
        || source
            .split_once(':')
            .is_some_and(|(host, _)| !host.is_empty() && !host.contains('/'));
    ensure!(
        url,
        InvalidArgument,
        "Specify a Git URL or an absolute repository path."
    );
    url_rules(source)
}

fn url_rules(source: &str) -> Result<()> {
    if let Some((scheme, rest)) = source.split_once("://") {
        ensure!(
            matches!(scheme, "https" | "http" | "ssh" | "git" | "file"),
            InvalidArgument,
            "Unsupported Git URL scheme."
        );
        let authority = rest.split('/').next().unwrap_or("");
        ensure!(
            !authority.is_empty() || scheme == "file",
            InvalidArgument,
            "The Git URL has no host."
        );
        if matches!(scheme, "https" | "http") {
            ensure!(
                !authority.contains('@') && !rest.contains(['?', '#']),
                InvalidArgument,
                "Do not put a user name or token in an HTTP URL; use a Git credential helper (or the system keychain) and the plain repository URL."
            );
        } else if scheme == "ssh" {
            ensure!(
                !authority.split('@').next().unwrap_or("").contains(':')
                    || !authority.contains('@'),
                InvalidArgument,
                "Do not put a password in an SSH URL; use an SSH key or agent."
            );
        }
    } else {
        ensure!(
            !source.contains("::"),
            InvalidArgument,
            "Git remote helpers are not supported by this check."
        );
        ensure!(
            source.contains(':') || source.starts_with('/') || source.starts_with('.'),
            InvalidArgument,
            "Specify a full Git URL or a repository path."
        );
    }
    Ok(())
}

pub(crate) fn failure(stderr: &str) -> &'static str {
    let message = stderr.to_lowercase();
    if message.contains("host key verification failed")
        || message.contains("remote host identification has changed")
    {
        "SSH host key verification failed. Check the server key and set up known_hosts with plain SSH; host verification is never disabled."
    } else if message.contains("permission denied")
        && (message.contains("publickey")
            || message.contains("password")
            || message.contains("keyboard-interactive"))
    {
        "SSH denied access. Check the key in ~/.ssh/config, that the public key is added to the Git host, and the repository permissions. If the key is encrypted, unlock it beforehand with ssh-add."
    } else if message.contains("could not resolve")
        || message.contains("name or service not known")
        || message.contains("nodename nor servname")
    {
        "Could not resolve the Git server name. Check the address, DNS and the network or VPN connection."
    } else if message.contains("timed out")
        || message.contains("failed to connect")
        || message.contains("connection refused")
        || message.contains("network is unreachable")
    {
        "The Git server is unreachable. Check the address, port, VPN and connection."
    } else if message.contains("certificate") || message.contains("ssl") {
        "Could not verify the Git server's TLS certificate. Check the trusted certificates and proxy settings."
    } else if message.contains("repository not found")
        || message.contains("does not appear to be a git repository")
        || message.contains("not found")
    {
        "Repository not found, or hidden for lack of access. Check the URL or path and the account's access; a private repository may look like a missing one."
    } else if message.contains("authentication")
        || message.contains("could not read username")
        || message.contains("could not read password")
        || message.contains("terminal prompts disabled")
        || message.contains("403")
        || message.contains("401")
        || message.contains("access denied")
    {
        "Git was not authorized or lacks permissions. Set up a credential helper (for example the macOS Keychain or Git Credential Manager), sign in to the Git host and check again. GitHub over HTTPS needs a token or OAuth, not the account password."
    } else {
        "Git could not read the repository. Check the address and authorization with a plain git ls-remote. The detailed stderr is hidden because it may contain secrets."
    }
}

/// Does not clone, fetch, create a workspace, or store credentials in PM.
/// A successful check also covers an empty repository with no refs.
pub fn check(source: &str, options: &Options) -> Result<AuthCheck> {
    validate(source)?;
    let output = transport::ls_remote(
        source,
        options.custom_ssh,
        "cannot start git; check the Git installation and PATH",
    )?;
    if !output.success {
        fail!(Auth, "{}", failure(&output.stderr));
    }
    Ok(AuthCheck {
        source: source.into(),
        message: "Read access confirmed (git ls-remote).\nNothing was installed. plugin.json, release tags and push access were not checked.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnoses_failures_without_echoing_secrets() {
        assert!(failure("Permission denied (publickey).").contains("SSH"));
        assert!(failure("Host key verification failed.").contains("host key"));
        assert!(
            failure(
                "could not read Username for https://secret@example.com: terminal prompts disabled"
            )
            .contains("credential helper")
        );
        assert!(!failure("fatal: token=supersecret").contains("supersecret"));
        assert!(failure("Repository not found").contains("lack of access"));
        assert!(failure("Could not resolve host").contains("DNS"));
    }
    #[test]
    fn rejects_embedded_credentials_before_starting_git() {
        for value in [
            "https://user:secret@example.com/repo",
            "https://example.com/repo?token=secret",
            "ssh://user:secret@example.com/repo",
            "--upload-pack=bad",
            "ext::bad",
        ] {
            assert!(validate(value).is_err(), "{value}");
        }
        for value in [
            "git@github.com:org/repo.git",
            "ssh://git@example.com:2222/repo.git",
            "https://example.com/repo.git",
        ] {
            assert!(validate(value).is_ok());
        }
    }
    #[test]
    fn recorded_sources_are_urls_or_absolute_paths_without_helpers() {
        for ok in [
            "/abs/repo",
            "/abs/missing.git",
            "https://git.example/acme/hello.git",
            "ssh://git@git.example/acme/hello.git",
            "git@git.example:acme/hello.git",
            "file:///abs/repo",
        ] {
            assert!(validate_recorded(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "fd::5",
            "ext::sh -c id",
            "foo/bar",
            "./foo",
            "../foo",
            "foo",
            "-uhelp",
            "ftp://host/x",
            "https://user:token@git.example/x.git",
            "/abs/with\nnewline",
        ] {
            let error = validate_recorded(bad).unwrap_err();
            assert_eq!(error.kind(), "invalid_argument", "{bad}");
        }
    }

    #[test]
    fn validate_covers_every_rule() {
        let cases: &[(&str, Option<&str>)] = &[
            ("", Some("Specify a Git URL")),
            ("-oProxyCommand=x", Some("Specify a Git URL")),
            ("https://example.com/repo\n.git", Some("Specify a Git URL")),
            (
                "ftp://example.com/repo.git",
                Some("Unsupported Git URL scheme"),
            ),
            ("https:///repo.git", Some("has no host")),
            ("ssh:///repo.git", Some("has no host")),
            (
                "https://token@example.com/repo.git",
                Some("Do not put a user name or token"),
            ),
            (
                "https://example.com/repo.git#frag",
                Some("Do not put a user name or token"),
            ),
            (
                "ssh://git:pw@example.com/repo.git",
                Some("Do not put a password"),
            ),
            ("ext::sh -c x", Some("remote helpers")),
            ("repo-without-scheme", Some("full Git URL")),
            ("file:///srv/repo.git", None),
            ("http://example.com/repo.git", None),
            ("git://example.com/repo.git", None),
            ("ssh://example.com/repo.git", None),
            ("ssh://git@example.com/repo.git", None),
            ("/srv/missing/repo.git", None),
            ("./relative/repo", None),
        ];
        for (source, expected) in cases {
            match (validate(source), expected) {
                (Ok(()), None) => {}
                (Err(error), Some(text)) => {
                    assert_eq!(error.kind(), "invalid_argument", "{source}");
                    assert!(error.to_string().contains(text), "{source}: {error}");
                    // The source (and any secret in it) is never echoed.
                    assert!(!error.to_string().contains("pw@"), "{error}");
                }
                (result, _) => panic!("{source}: unexpected {result:?}"),
            }
        }
        // An existing local path is accepted even without a leading '/' or '.'.
        let dir = crate::test_support::TempDir::new("auth");
        assert!(validate(dir.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn failure_maps_every_category_and_hides_stderr() {
        let cases = [
            (
                "WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!",
                "host key",
            ),
            (
                "git@x: Permission denied (keyboard-interactive).",
                "SSH denied access",
            ),
            ("ssh: Could not resolve hostname x", "DNS"),
            ("nodename nor servname provided", "DNS"),
            ("Connection timed out", "unreachable"),
            (
                "connect to host x port 22: Connection refused",
                "unreachable",
            ),
            ("SSL certificate problem: self signed certificate", "TLS"),
            (
                "fatal: 'x' does not appear to be a git repository",
                "not found",
            ),
            ("The requested URL returned error: 403", "not authorized"),
            ("remote: HTTP Basic: Access denied", "not authorized"),
            (
                "fatal: Authentication failed for 'https://x'",
                "not authorized",
            ),
            ("something unexpected: secret-token-123", "stderr is hidden"),
        ];
        for (stderr, text) in cases {
            let message = failure(stderr);
            assert!(message.contains(text), "{stderr}: {message}");
            assert!(!message.contains("secret-token-123"));
        }
    }

    #[test]
    fn check_reads_local_repositories_without_cloning() {
        use crate::test_support::{Author, TempDir};
        let dir = TempDir::new("auth");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        let before = crate::test_support::tree_hash(dir.path());
        let options = Options {
            custom_ssh: true,
            ..Default::default()
        };
        let result = check(author.source(), &options).unwrap();
        assert_eq!(result.source, author.source());
        assert!(result.message.starts_with("Read access confirmed"));
        assert_eq!(crate::test_support::tree_hash(dir.path()), before);

        // An empty repository without refs is still readable.
        let empty = dir.join("empty");
        std::fs::create_dir(&empty).unwrap();
        crate::test_support::git(&empty, &["init", "-q", "--bare"]);
        assert!(check(empty.to_str().unwrap(), &Options::default()).is_ok());

        let error = check(dir.join("missing").to_str().unwrap(), &options).unwrap_err();
        assert_eq!(error.kind(), "auth");
        assert!(error.to_string().contains("not found"), "{error}");
        assert_eq!(check("", &options).unwrap_err().kind(), "invalid_argument");
    }

    #[test]
    fn access_check_is_bounded() {
        // A server that accepts the connection and never answers: git waits for the
        // ref advertisement forever unless the check kills it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let source = format!("git://{}/silent.git", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (connection, _) = listener.accept().unwrap();
            accepted_tx.send(()).unwrap();
            let _ = release_rx.recv();
            drop(connection);
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(check(&source, &Options::default()));
        });
        // Only a guard against hanging the test run; the check must end on its own limit.
        let result = receiver
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("auth check against a silent server did not finish");
        accepted_rx
            .try_recv()
            .expect("git never connected to the silent server");
        drop(release_tx);
        server.join().unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(error.to_string().contains("timed out after 10s"), "{error}");
    }
}
