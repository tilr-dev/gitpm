//! Typed errors with stable kinds and human-readable messages.
use crate::LostWork;
use std::fmt::Display;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Every variant renders a complete human message via `Display`.
/// `kind()` is a stable machine-readable identifier for callers (CLI, FFI, UI).
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("plugin is not installed")]
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
    #[error("no previous version")]
    NoPreviousVersion { id: String },
    #[error("{0}")]
    NoReleases(String),
    /// The path for the ID is not a plugin repository (also for case variants of an ID).
    #[error("plugin repository not found (a namespace directory is not a plugin)")]
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
