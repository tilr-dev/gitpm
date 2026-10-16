//! Typed errors with stable kinds and human-readable messages.
use crate::LostWork;
use std::fmt::Display;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every variant renders a complete human message via `Display`.
/// `kind()` is a stable machine-readable identifier for callers (CLI, FFI, UI).
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("plugin {id} is not installed")]
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
    #[error("{0}")]
    InvalidManifest(String),
    #[error("{0}")]
    InvalidVersion(String),
    /// Bad caller input: refs, sources, URLs, conflicting options, destinations.
    #[error("{0}")]
    InvalidArgument(String),
    /// Existing release/tag/version rules forbid the operation.
    #[error("{0}")]
    VersionConflict(String),
    #[error("no previous version of {id}")]
    NoPreviousVersion { id: String },
    #[error("{0}")]
    NoReleases(String),
    /// The path for the ID is not a plugin repository (also for case variants of an ID).
    #[error("plugin repository {id} not found (a namespace directory is not a plugin)")]
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
    /// Other installed plugins depend on this one; pass `force` to proceed anyway.
    #[error("{message}")]
    HasDependents {
        id: String,
        message: String,
        dependents: Vec<String>,
    },
    /// Some consumers do not accept the version the new provider declares; confirm
    /// to replace anyway. `consumers` are the unsatisfied ones.
    #[error("{message}")]
    IncompatibleReplacement {
        id: String,
        message: String,
        consumers: Vec<String>,
    },
    /// Malformed JSON/state data.
    #[error("{0}")]
    InvalidData(String),
    #[error("{0}")]
    Io(String),
}

impl Error {
    /// Stable snake_case identifier; never changes for an existing variant.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::NotInstalled { .. } => "not_installed",
            Error::AlreadyInstalled { .. } => "already_installed",
            Error::LocalChanges { .. } => "local_changes",
            Error::Locked(_) => "locked",
            Error::InvalidId(_) => "invalid_id",
            Error::InvalidManifest(_) => "invalid_manifest",
            Error::InvalidVersion(_) => "invalid_version",
            Error::InvalidArgument(_) => "invalid_argument",
            Error::VersionConflict(_) => "version_conflict",
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
    ($cond:expr, $variant:ident, $($fmt:tt)+) => {
        if !$cond {
            return Err($crate::Error::$variant(format!($($fmt)+)));
        }
    };
}
/// `fail!(Variant, "format", args...)` returns `Error::Variant(message)` immediately.
macro_rules! fail {
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
            (Error::InvalidManifest(s()), "invalid_manifest"),
            (Error::InvalidVersion(s()), "invalid_version"),
            (Error::InvalidArgument(s()), "invalid_argument"),
            (Error::VersionConflict(s()), "version_conflict"),
            (
                Error::NoPreviousVersion { id: "a/b".into() },
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
            | Error::InvalidManifest(_)
            | Error::InvalidVersion(_)
            | Error::InvalidArgument(_)
            | Error::VersionConflict(_)
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
            | Error::InvalidData(_)
            | Error::Io(_) => {}
        }
    }

    #[test]
    fn every_variant_has_its_stable_kind() {
        let all = all();
        assert_eq!(all.len(), 23);
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
            "plugin a/b is not installed"
        );
        assert_eq!(
            Error::NoPreviousVersion { id: "a/b".into() }.to_string(),
            "no previous version of a/b"
        );
        assert_eq!(
            Error::RepositoryNotFound { id: "a/b".into() }.to_string(),
            "plugin repository a/b not found (a namespace directory is not a plugin)"
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
