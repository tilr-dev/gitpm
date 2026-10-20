//! Typed errors with stable kinds and human-readable messages.
use crate::LostWork;
use std::fmt::Display;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every variant renders a complete human message via `Display`.
/// `kind()` is a stable machine-readable identifier for callers (CLI, FFI, UI).
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("package {id} is not installed")]
    NotInstalled { id: String },
    /// The ID is registered or its repository already exists in the workspace.
    #[error("{message}")]
    AlreadyInstalled { id: String, message: String },
    /// Removing or replacing the repository would lose local work.
    #[error("{message}")]
    LocalChanges {
        id: String,
        message: String,
        items: Vec<LostWork>,
    },
    /// A workspace or catalog lock is held by someone else.
    #[error("{0}")]
    Locked(String),
    #[error("{0}")]
    InvalidId(String),
    #[error("{message}")]
    InvalidManifest { id: Option<String>, message: String },
    #[error("{0}")]
    InvalidVersion(String),
    /// Bad caller input: refs, sources, URLs, conflicting options, destinations.
    #[error("{0}")]
    InvalidArgument(String),
    /// Existing release/tag/version rules forbid the operation.
    #[error("{message}")]
    VersionConflict { id: Option<String>, message: String },
    /// No previous revision to roll back to, or (`commit`) the previous revision's commit
    /// is no longer in the clone (pruned after the origin deleted or moved its tag).
    #[error("{}", match commit {
        Some(commit) => format!("previous revision {commit} of {id} is no longer available"),
        None => format!("no previous version of {id}"),
    })]
    NoPreviousVersion { id: String, commit: Option<String> },
    #[error("{0}")]
    NoReleases(String),
    /// The path for the ID is not a package repository (also for case variants of an ID).
    #[error("package repository {id} not found (a namespace directory is not a package)")]
    RepositoryNotFound { id: String },
    /// Workspace layout or path conflicts.
    #[error("{0}")]
    Workspace(String),
    /// Catalog data, lookup or write problems.
    #[error("{0}")]
    Catalog(String),
    #[error("{0}")]
    Git(String),
    #[error("{0}")]
    Http(String),
    /// Git repository access check failed; message is a user-facing diagnosis.
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    Unsupported(String),
    /// Dependencies cannot be satisfied; the message explains why (solver report).
    #[error("{0}")]
    DependencyConflict(String),
    /// Other installed packages would break (their dependency resolves to `id`, or to
    /// it through a replacement the original does not take over); pass `force` to
    /// proceed anyway. `lost_work` is the local work an uninstall would also lose (the
    /// plan's), so one refusal tells everything; empty for other operations.
    #[error("{message}")]
    HasDependents {
        id: String,
        message: String,
        dependents: Vec<String>,
        lost_work: Vec<LostWork>,
    },
    /// Some consumers do not accept the version the new provider declares; confirm
    /// to replace anyway. `consumers` are the unsatisfied ones.
    #[error("{message}")]
    IncompatibleReplacement {
        id: String,
        message: String,
        consumers: Vec<String>,
    },
    /// `sync_dist` committed what it could but left active packages without their
    /// deployed folder; `sync` is the report (its `skipped` lists them with the reason).
    #[error("{message}")]
    NotDeployed {
        message: String,
        sync: Box<crate::DistSync>,
    },
    /// Malformed JSON/state data.
    #[error("{0}")]
    InvalidData(String),
    #[error("{0}")]
    Io(String),
}

impl Error {
    /// A manifest error outside a known package operation.
    pub(crate) fn invalid_manifest(message: String) -> Self {
        Self::InvalidManifest { id: None, message }
    }

    /// A version conflict outside a known package operation.
    pub(crate) fn version_conflict(message: String) -> Self {
        Self::VersionConflict { id: None, message }
    }

    /// Identify the package whose update or rollback was refused.
    pub(crate) fn with_package(mut self, package: &str) -> Self {
        if let Self::InvalidManifest { id, message } | Self::VersionConflict { id, message } =
            &mut self
            && id.is_none()
        {
            *id = Some(package.into());
            *message = format!("{package}: {message}");
        }
        self
    }

    /// Put the clone's ID and its lost-work items into a `LocalChanges` message.
    /// Applied once, where update, update all and rollback return, so the CLI
    /// prints the message alone.
    pub(crate) fn listing_lost_work(mut self) -> Self {
        if let Self::LocalChanges { id, message, items } = &mut self {
            *message = format!("{id}: {message}");
            for item in items {
                message.push_str(&format!("; {}", item.describe()));
            }
        }
        self
    }

    /// Stable snake_case identifier; never changes for an existing variant.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::NotInstalled { .. } => "not_installed",
            Error::AlreadyInstalled { .. } => "already_installed",
            Error::LocalChanges { .. } => "local_changes",
            Error::Locked(_) => "locked",
            Error::InvalidId(_) => "invalid_id",
            Error::InvalidManifest { .. } => "invalid_manifest",
            Error::InvalidVersion(_) => "invalid_version",
            Error::InvalidArgument(_) => "invalid_argument",
            Error::VersionConflict { .. } => "version_conflict",
            Error::NoPreviousVersion { .. } => "no_previous_version",
            Error::NoReleases(_) => "no_releases",
            Error::RepositoryNotFound { .. } => "repository_not_found",
            Error::Workspace(_) => "workspace",
            Error::Catalog(_) => "catalog",
            Error::Git(_) => "git",
            Error::Http(_) => "http",
            Error::Auth(_) => "auth",
            Error::Unsupported(_) => "unsupported",
            Error::DependencyConflict(_) => "dependency_conflict",
            Error::HasDependents { .. } => "has_dependents",
            Error::IncompatibleReplacement { .. } => "incompatible_replacement",
            Error::NotDeployed { .. } => "not_deployed",
            Error::InvalidData(_) => "invalid_data",
            Error::Io(_) => "io",
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::InvalidData(error.to_string())
    }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(error: std::string::FromUtf8Error) -> Self {
        Error::InvalidData(error.to_string())
    }
}
impl From<std::num::ParseIntError> for Error {
    fn from(error: std::num::ParseIntError) -> Self {
        Error::InvalidVersion(error.to_string())
    }
}
impl From<std::path::StripPrefixError> for Error {
    fn from(error: std::path::StripPrefixError) -> Self {
        Error::Io(error.to_string())
    }
}
impl From<std::time::SystemTimeError> for Error {
    fn from(error: std::time::SystemTimeError) -> Self {
        Error::Io(error.to_string())
    }
}

/// `ensure!(condition, Variant, "format", args...)` returns `Error::Variant(message)`.
macro_rules! ensure {
    ($cond:expr, InvalidManifest, $($fmt:tt)+) => {
        if !$cond { return Err($crate::Error::invalid_manifest(format!($($fmt)+))); }
    };
    ($cond:expr, VersionConflict, $($fmt:tt)+) => {
        if !$cond { return Err($crate::Error::version_conflict(format!($($fmt)+))); }
    };
    ($cond:expr, $variant:ident, $($fmt:tt)+) => {
        if !$cond {
            return Err($crate::Error::$variant(format!($($fmt)+)));
        }
    };
}
/// `fail!(Variant, "format", args...)` returns `Error::Variant(message)` immediately.
macro_rules! fail {
    (InvalidManifest, $($fmt:tt)+) => {
        return Err($crate::Error::invalid_manifest(format!($($fmt)+)))
    };
    (VersionConflict, $($fmt:tt)+) => {
        return Err($crate::Error::version_conflict(format!($($fmt)+)))
    };
    ($variant:ident, $($fmt:tt)+) => {
        return Err($crate::Error::$variant(format!($($fmt)+)))
    };
}
pub(crate) use {ensure, fail};

/// anyhow-style context: `"{context}: {cause}"` wrapped into the chosen variant.
pub(crate) trait Context<T> {
    fn context(self, make: fn(String) -> Error, context: impl Display) -> Result<T>;
}
impl<T, E: Display> Context<T> for std::result::Result<T, E> {
    fn context(self, make: fn(String) -> Error, context: impl Display) -> Result<T> {
        self.map_err(|error| make(format!("{context}: {error}")))
    }
}
/// Option to error with a fixed message.
pub(crate) trait Require<T> {
    fn require(self, make: fn(String) -> Error, message: impl Display) -> Result<T>;
}
impl<T> Require<T> for Option<T> {
    fn require(self, make: fn(String) -> Error, message: impl Display) -> Result<T> {
        self.ok_or_else(|| make(message.to_string()))
    }
}

impl From<gitpm_transport::Error> for Error {
    fn from(error: gitpm_transport::Error) -> Self {
        use gitpm_transport::Error as TransportError;
        match error {
            TransportError::Git(s) => Self::Git(s),
            TransportError::Http(s) => Self::Http(s),
            TransportError::Auth(s) => Self::Auth(s),
            TransportError::Io(s) => Self::Io(s),
            TransportError::InvalidData(s) => Self::InvalidData(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// One value of every variant with the kind it must report.
    fn all() -> Vec<(Error, &'static str)> {
        let s = || "message".to_owned();
        vec![
            (Error::NotInstalled { id: "a/b".into() }, "not_installed"),
            (
                Error::AlreadyInstalled {
                    id: "a/b".into(),
                    message: s(),
                },
                "already_installed",
            ),
            (
                Error::LocalChanges {
                    id: "a/b".into(),
                    message: s(),
                    items: vec![],
                },
                "local_changes",
            ),
            (Error::Locked(s()), "locked"),
            (Error::InvalidId(s()), "invalid_id"),
            (Error::invalid_manifest(s()), "invalid_manifest"),
            (Error::InvalidVersion(s()), "invalid_version"),
            (Error::InvalidArgument(s()), "invalid_argument"),
            (Error::version_conflict(s()), "version_conflict"),
            (
                Error::NoPreviousVersion {
                    id: "a/b".into(),
                    commit: None,
                },
                "no_previous_version",
            ),
            (Error::NoReleases(s()), "no_releases"),
            (
                Error::RepositoryNotFound { id: "a/b".into() },
                "repository_not_found",
            ),
            (Error::Workspace(s()), "workspace"),
            (Error::Catalog(s()), "catalog"),
            (Error::Git(s()), "git"),
            (Error::Http(s()), "http"),
            (Error::Auth(s()), "auth"),
            (Error::Unsupported(s()), "unsupported"),
            (Error::DependencyConflict(s()), "dependency_conflict"),
            (
                Error::HasDependents {
                    id: "a/b".into(),
                    message: s(),
                    dependents: vec![],
                    lost_work: vec![],
                },
                "has_dependents",
            ),
            (
                Error::IncompatibleReplacement {
                    id: "a/b".into(),
                    message: s(),
                    consumers: vec![],
                },
                "incompatible_replacement",
            ),
            (
                Error::NotDeployed {
                    message: s(),
                    sync: Box::default(),
                },
                "not_deployed",
            ),
            (Error::InvalidData(s()), "invalid_data"),
            (Error::Io(s()), "io"),
        ]
    }
    /// Fails to compile when a variant is added, reminding to extend `all()`.
    fn covered(error: &Error) {
        match error {
            Error::NotInstalled { .. }
            | Error::AlreadyInstalled { .. }
            | Error::LocalChanges { .. }
            | Error::Locked(_)
            | Error::InvalidId(_)
            | Error::InvalidManifest { .. }
            | Error::InvalidVersion(_)
            | Error::InvalidArgument(_)
            | Error::VersionConflict { .. }
            | Error::NoPreviousVersion { .. }
            | Error::NoReleases(_)
            | Error::RepositoryNotFound { .. }
            | Error::Workspace(_)
            | Error::Catalog(_)
            | Error::Git(_)
            | Error::Http(_)
            | Error::Auth(_)
            | Error::Unsupported(_)
            | Error::DependencyConflict(_)
            | Error::HasDependents { .. }
            | Error::IncompatibleReplacement { .. }
            | Error::NotDeployed { .. }
            | Error::InvalidData(_)
            | Error::Io(_) => {}
        }
    }

    #[test]
    fn package_errors_name_the_package() {
        for error in [
            Error::invalid_manifest("manifest mismatch".into()),
            Error::version_conflict("history diverged".into()),
        ] {
            let kind = error.kind();
            let message = error.to_string();
            assert!(matches!(
                &error,
                Error::InvalidManifest { id: None, .. } | Error::VersionConflict { id: None, .. }
            ));
            let scoped = error.with_package("dan/demo");
            assert_eq!(scoped.kind(), kind);
            assert_eq!(scoped.to_string(), format!("dan/demo: {message}"));
            assert!(matches!(&scoped,
                Error::InvalidManifest { id: Some(id), .. } | Error::VersionConflict { id: Some(id), .. }
                if id == "dan/demo"));
        }
        let error = Error::LocalChanges {
            id: "dan/demo".into(),
            message: "local work blocks update".into(),
            items: vec![LostWork::Untracked {
                path: "extra.js".into(),
            }],
        }
        .with_package("dan/demo")
        .listing_lost_work();
        assert_eq!(
            error.to_string(),
            "dan/demo: local work blocks update; untracked: extra.js"
        );
    }

    #[test]
    fn every_variant_has_its_stable_kind() {
        let all = all();
        assert_eq!(all.len(), 24);
        for (error, kind) in &all {
            covered(error);
            assert_eq!(error.kind(), *kind, "{error:?}");
        }
        let unique: BTreeSet<_> = all.iter().map(|(e, _)| e.kind()).collect();
        assert_eq!(unique.len(), all.len(), "kinds must be unique");
        for (_, kind) in &all {
            assert!(
                kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{kind} is not snake_case"
            );
        }
    }

    #[test]
    fn readme_lists_exactly_the_error_kinds() {
        let readme = include_str!("../../../README.md");
        let section = readme
            .split("### Error kinds")
            .nth(1)
            .expect("README has an Error kinds section");
        let section = section.split("\n#").next().unwrap();
        let listed: BTreeSet<_> = section
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect();
        let kinds: BTreeSet<_> = all().iter().map(|(e, _)| e.kind().to_owned()).collect();
        assert_eq!(listed, kinds);
    }

    #[test]
    fn display_is_the_complete_message() {
        assert_eq!(
            Error::NotInstalled { id: "a/b".into() }.to_string(),
            "package a/b is not installed"
        );
        assert_eq!(
            Error::NoPreviousVersion {
                id: "a/b".into(),
                commit: None,
            }
            .to_string(),
            "no previous version of a/b"
        );
        assert_eq!(
            Error::RepositoryNotFound { id: "a/b".into() }.to_string(),
            "package repository a/b not found (a namespace directory is not a package)"
        );
        // Every other variant renders exactly its message.
        for (error, _) in all() {
            if !matches!(
                error,
                Error::NotInstalled { .. }
                    | Error::NoPreviousVersion { .. }
                    | Error::RepositoryNotFound { .. }
            ) {
                assert_eq!(error.to_string(), "message", "{error:?}");
            }
        }
    }

    /// String literals of `source` outside comments and the test module.
    fn literals(source: &str) -> Vec<String> {
        let code = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let chars: Vec<char> = code.chars().collect();
        let (mut out, mut i) = (Vec::new(), 0);
        while i < chars.len() {
            match chars[i] {
                '/' if chars.get(i + 1) == Some(&'/') => {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                }
                // A char literal ('"', '\'', 'x'); a lifetime has no closing quote.
                '\'' if chars.get(i + 1) == Some(&'\\') => i += 4,
                '\'' if chars.get(i + 2) == Some(&'\'') => i += 3,
                'r' if chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'"') => {
                    let start = i + 3;
                    i = start;
                    while !(chars[i] == '"' && chars.get(i + 1) == Some(&'#')) {
                        i += 1;
                    }
                    out.push(chars[start..i].iter().collect());
                    i += 2;
                }
                '"' => {
                    let mut text = String::new();
                    i += 1;
                    while chars[i] != '"' {
                        if chars[i] == '\\' {
                            i += 1;
                        }
                        text.push(chars[i]);
                        i += 1;
                    }
                    out.push(text);
                    i += 1;
                }
                _ => i += 1,
            }
        }
        out
    }

    /// Library messages are neutral: no CLI command, flag or hosting service (the CLI
    /// adds command hints per error kind). Git commands quoted in messages (`git …`) and
    /// the README a new package repository gets are not messages.
    #[test]
    fn messages_name_no_cli_command() {
        let scaffold = include_str!("scaffold.rs");
        let readme_start = scaffold.find("let install = if release").unwrap();
        let readme_end = scaffold.find("git(&directory, &[\"init\"").unwrap();
        let scaffold = format!("{}{}", &scaffold[..readme_start], &scaffold[readme_end..]);
        let sources = [
            ("auth.rs", include_str!("auth.rs")),
            ("catalog.rs", include_str!("catalog.rs")),
            ("check.rs", include_str!("check.rs")),
            ("deps.rs", include_str!("deps.rs")),
            ("error.rs", include_str!("error.rs")),
            ("lib.rs", include_str!("lib.rs")),
            (
                "transport.rs",
                include_str!("../../gitpm-transport/src/transport.rs"),
            ),
            ("identity.rs", include_str!("identity.rs")),
            ("list.rs", include_str!("list.rs")),
            ("lock.rs", include_str!("lock.rs")),
            ("lost_work.rs", include_str!("lost_work.rs")),
            ("model.rs", include_str!("model.rs")),
            ("packages.rs", include_str!("packages.rs")),
            ("releases.rs", include_str!("releases.rs")),
            ("scaffold.rs", scaffold.as_str()),
            ("txn.rs", include_str!("txn.rs")),
            ("upgrade.rs", include_str!("upgrade.rs")),
            ("workspace.rs", include_str!("workspace.rs")),
            ("workspace/clones.rs", include_str!("workspace/clones.rs")),
            ("workspace/fork.rs", include_str!("workspace/fork.rs")),
        ];
        let commands = [
            "init",
            "fork",
            "remote",
            "auth",
            "info",
            "catalog",
            "commit",
            "install",
            "list",
            "check",
            "update",
            "rollback",
            "enable",
            "disable",
            "forget",
            "restore",
            "uninstall",
            "replace",
            "sync-dist",
            "register",
        ];
        let mut found = Vec::new();
        for (file, source) in sources {
            for text in literals(source) {
                // Git arguments (`--format=…`) and quoted git commands are not prose.
                let flag = text.contains(' ')
                    && !text.starts_with("git ")
                    && !text.starts_with("--")
                    && text.match_indices("--").any(|(at, _)| {
                        text[at + 2..].starts_with(|c: char| c.is_ascii_lowercase())
                    });
                // `gitpm restore`, `` `restore ``, or advice such as "use restore"/"run update".
                let command = commands.iter().any(|c| {
                    text.contains(&format!("gitpm {c}"))
                        || text.contains(&format!("`{c}"))
                        || ["use ", "run "].iter().any(|verb| {
                            text.match_indices(&format!("{verb}{c}")).any(|(at, m)| {
                                !text[at + m.len()..]
                                    .starts_with(|ch: char| ch.is_ascii_alphanumeric())
                            })
                        })
                });
                if flag || command || text.contains("R2") || text.contains("catalog-update") {
                    found.push(format!("{file}: {text}"));
                }
            }
        }
        assert!(
            found.is_empty(),
            "messages naming CLI commands:\n{}",
            found.join("\n")
        );
        assert!(literals("x(\"a --b\", '\"', r#\"c\"#) // \"d\"").len() == 2);
        // The checks catch the advice the messages used to give.
        let flagged = |text: &str| {
            commands.iter().any(|c| {
                ["use ", "run "].iter().any(|verb| {
                    text.match_indices(&format!("{verb}{c}")).any(|(at, m)| {
                        !text[at + m.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
                    })
                })
            })
        };
        assert!(flagged("kept; use restore, or uninstall it first"));
        assert!(!flagged("use the default branch") && !flagged("use restored files"));
    }

    #[test]
    fn conversions_choose_the_documented_kind() {
        let io: Error = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert_eq!((io.kind(), io.to_string().as_str()), ("io", "gone"));
        let json: Error = serde_json::from_str::<u8>("{").unwrap_err().into();
        assert_eq!(json.kind(), "invalid_data");
        let utf8: Error = String::from_utf8(vec![0xff]).unwrap_err().into();
        assert_eq!(utf8.kind(), "invalid_data");
        let int: Error = "x".parse::<u64>().unwrap_err().into();
        assert_eq!(int.kind(), "invalid_version");
        let prefix: Error = std::path::Path::new("/a")
            .strip_prefix("/b")
            .unwrap_err()
            .into();
        assert_eq!(prefix.kind(), "io");
        let time: Error = std::time::UNIX_EPOCH
            .duration_since(std::time::SystemTime::now())
            .unwrap_err()
            .into();
        assert_eq!(time.kind(), "io");
    }

    fn checked(value: u32) -> Result<u32> {
        ensure!(value < 10, InvalidArgument, "too big: {value}");
        if value == 7 {
            fail!(VersionConflict, "unlucky {}", value);
        }
        Ok(value)
    }
    #[test]
    fn helper_macros_and_traits_build_variants() {
        assert_eq!(checked(3).unwrap(), 3);
        let error = checked(12).unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("invalid_argument", "too big: 12".into())
        );
        let error = checked(7).unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("version_conflict", "unlucky 7".into())
        );

        let failed: std::result::Result<(), &str> = Err("cause");
        let error = failed.context(Error::Git, "cannot start git").unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("git", "cannot start git: cause".into())
        );
        let ok: std::result::Result<u8, &str> = Ok(1);
        assert_eq!(ok.context(Error::Git, "unused").unwrap(), 1);

        let error = None::<u8>.require(Error::Catalog, "missing").unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("catalog", "missing".into())
        );
        assert_eq!(Some(2).require(Error::Catalog, "unused").unwrap(), 2);
    }
}
