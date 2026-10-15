//! Check repository read access using the user's existing Git credentials.
use crate::error::{Result, ensure, fail};
use crate::exec;
use serde::Serialize;
use std::path::Path;

/// Options for [`check`]. The library never reads the environment itself.
#[derive(Debug, Clone, Default)]
pub struct AuthOptions {
    /// The caller's environment already selects an SSH transport (`GIT_SSH_COMMAND` or
    /// `GIT_SSH`). When false and Git has no `core.sshCommand`, a noninteractive SSH
    /// command with strict host key checking is used.
    pub custom_ssh: bool,
}

/// Successful read-access check.
#[derive(Debug, Clone, Serialize)]
pub struct AuthCheck {
    pub source: String,
    /// Human-readable confirmation (Russian, as shown by the stand CLI).
    pub message: String,
}

pub(crate) const NONINTERACTIVE_SSH: &str =
    "ssh -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=yes";

fn validate(source: &str) -> Result<()> {
    ensure!(
        !source.is_empty() && !source.starts_with('-') && !source.chars().any(char::is_control),
        InvalidArgument,
        "Укажи Git URL или локальный путь."
    );
    if Path::new(source).exists() {
        return Ok(());
    }
    if let Some((scheme, rest)) = source.split_once("://") {
        ensure!(
            matches!(scheme, "https" | "http" | "ssh" | "git" | "file"),
            InvalidArgument,
            "Неподдерживаемый протокол Git URL."
        );
        let authority = rest.split('/').next().unwrap_or("");
        ensure!(
            !authority.is_empty() || scheme == "file",
            InvalidArgument,
            "В Git URL не указан хост."
        );
        if matches!(scheme, "https" | "http") {
            ensure!(
                !authority.contains('@') && !rest.contains(['?', '#']),
                InvalidArgument,
                "Не передавай логин/токен в HTTP URL; используй Git credential helper/Keychain и чистый URL репозитория."
            );
        } else if scheme == "ssh" {
            ensure!(
                !authority.split('@').next().unwrap_or("").contains(':')
                    || !authority.contains('@'),
                InvalidArgument,
                "Не передавай пароль в SSH URL; используй SSH-ключ/agent."
            );
        }
    } else {
        ensure!(
            !source.contains("::"),
            InvalidArgument,
            "Git remote helpers не поддерживаются этой проверкой."
        );
        ensure!(
            source.contains(':') || source.starts_with('/') || source.starts_with('.'),
            InvalidArgument,
            "Укажи полный Git URL или путь к репозиторию."
        );
    }
    Ok(())
}

fn failure(stderr: &str) -> &'static str {
    let message = stderr.to_lowercase();
    if message.contains("host key verification failed")
        || message.contains("remote host identification has changed")
    {
        "Не удалось проверить SSH host key. Проверь ключ сервера и настрой known_hosts обычным SSH; проверка сервера не отключается."
    } else if message.contains("permission denied")
        && (message.contains("publickey")
            || message.contains("password")
            || message.contains("keyboard-interactive"))
    {
        "SSH не предоставил доступ. Проверь ключ в ~/.ssh/config, добавление публичного ключа в Git-хостинг и права на репозиторий. Если ключ зашифрован, заранее разблокируй его через ssh-add."
    } else if message.contains("could not resolve")
        || message.contains("name or service not known")
        || message.contains("nodename nor servname")
    {
        "Не удалось разрешить имя Git-сервера. Проверь адрес, DNS и подключение к сети/VPN."
    } else if message.contains("timed out")
        || message.contains("failed to connect")
        || message.contains("connection refused")
        || message.contains("network is unreachable")
    {
        "Git-сервер недоступен по сети. Проверь адрес, порт, VPN и подключение."
    } else if message.contains("certificate") || message.contains("ssl") {
        "Не удалось проверить TLS-сертификат Git-сервера. Проверь доверенные сертификаты и настройки корпоративного прокси."
    } else if message.contains("repository not found")
        || message.contains("does not appear to be a git repository")
        || message.contains("not found")
    {
        "Репозиторий не найден или скрыт из-за отсутствия прав. Проверь URL/путь и доступ аккаунта; приватный репозиторий может отвечать как несуществующий."
    } else if message.contains("authentication")
        || message.contains("could not read username")
        || message.contains("could not read password")
        || message.contains("terminal prompts disabled")
        || message.contains("403")
        || message.contains("401")
        || message.contains("access denied")
    {
        "Git не получил авторизацию или нужные права. Настрой credential helper (например macOS Keychain/Git Credential Manager), войди в Git-хостинг и повтори проверку. Для GitHub HTTPS нужен токен или OAuth, не пароль аккаунта."
    } else {
        "Git не смог прочитать репозиторий. Проверь адрес и авторизацию обычным git ls-remote. Подробный stderr скрыт: он может содержать секреты."
    }
}

/// Does not clone, fetch, create a workspace, or store credentials in PM.
/// A successful check also covers an empty repository with no refs.
pub fn check(source: &str, options: &AuthOptions) -> Result<AuthCheck> {
    validate(source)?;
    // Preserve explicitly configured SSH transports. Standard SSH remains
    // noninteractive, uses the user's config/agent and requires a known host.
    let custom_ssh = options.custom_ssh
        || exec::git_config_defined("core.sshCommand", "Не удалось запустить git")?;
    let output = exec::ls_remote(
        source,
        (!custom_ssh).then_some(NONINTERACTIVE_SSH),
        "Не удалось запустить git; проверь установку Git и PATH",
    )?;
    if !output.success {
        fail!(Auth, "{}", failure(&output.stderr));
    }
    Ok(AuthCheck {
        source: source.into(),
        message: "Доступ на чтение подтверждён (git ls-remote).\nНичего не установлено. Наличие plugin.json, релизных тегов и права на push не проверялись.".into(),
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
        assert!(failure("Repository not found").contains("отсутствия прав"));
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
    fn validate_covers_every_rule() {
        let cases: &[(&str, Option<&str>)] = &[
            ("", Some("Укажи Git URL")),
            ("-oProxyCommand=x", Some("Укажи Git URL")),
            ("https://example.com/repo\n.git", Some("Укажи Git URL")),
            (
                "ftp://example.com/repo.git",
                Some("Неподдерживаемый протокол"),
            ),
            ("https:///repo.git", Some("не указан хост")),
            ("ssh:///repo.git", Some("не указан хост")),
            (
                "https://token@example.com/repo.git",
                Some("Не передавай логин/токен"),
            ),
            (
                "https://example.com/repo.git#frag",
                Some("Не передавай логин/токен"),
            ),
            (
                "ssh://git:pw@example.com/repo.git",
                Some("Не передавай пароль"),
            ),
            ("ext::sh -c x", Some("remote helpers")),
            ("repo-without-scheme", Some("полный Git URL")),
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
                "SSH не предоставил",
            ),
            ("ssh: Could not resolve hostname x", "DNS"),
            ("nodename nor servname provided", "DNS"),
            ("Connection timed out", "недоступен по сети"),
            (
                "connect to host x port 22: Connection refused",
                "недоступен по сети",
            ),
            ("SSL certificate problem: self signed certificate", "TLS"),
            (
                "fatal: 'x' does not appear to be a git repository",
                "не найден",
            ),
            ("The requested URL returned error: 403", "авторизацию"),
            ("remote: HTTP Basic: Access denied", "авторизацию"),
            (
                "fatal: Authentication failed for 'https://x'",
                "авторизацию",
            ),
            (
                "something unexpected: secret-token-123",
                "Подробный stderr скрыт",
            ),
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
        let options = AuthOptions { custom_ssh: true };
        let result = check(author.source(), &options).unwrap();
        assert_eq!(result.source, author.source());
        assert!(result.message.starts_with("Доступ на чтение подтверждён"));
        assert_eq!(crate::test_support::tree_hash(dir.path()), before);

        // An empty repository without refs is still readable.
        let empty = dir.join("empty");
        std::fs::create_dir(&empty).unwrap();
        crate::test_support::git(&empty, &["init", "-q", "--bare"]);
        assert!(check(empty.to_str().unwrap(), &AuthOptions::default()).is_ok());

        let error = check(dir.join("missing").to_str().unwrap(), &options).unwrap_err();
        assert_eq!(error.kind(), "auth");
        assert!(error.to_string().contains("не найден"), "{error}");
        assert_eq!(check("", &options).unwrap_err().kind(), "invalid_argument");
    }
}
