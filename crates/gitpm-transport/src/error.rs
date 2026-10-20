use std::fmt::Display;
pub type Result<T> = std::result::Result<T, Error>;

/// Transport failures, converted by gitpm to its stable public error kinds.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Git(String),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    InvalidData(String),
}
impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Git(_) => "git",
            Self::Http(_) => "http",
            Self::Auth(_) => "auth",
            Self::Io(_) => "io",
            Self::InvalidData(_) => "invalid_data",
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::InvalidData(e.to_string())
    }
}
pub(crate) trait Context<T> {
    fn context(self, make: fn(String) -> Error, context: impl Display) -> Result<T>;
}
impl<T, E: Display> Context<T> for std::result::Result<T, E> {
    fn context(self, make: fn(String) -> Error, context: impl Display) -> Result<T> {
        self.map_err(|error| make(format!("{context}: {error}")))
    }
}
macro_rules! ensure {
    ($cond:expr, $variant:ident, $($fmt:tt)+) => {
        if !$cond { return Err($crate::Error::$variant(format!($($fmt)+))); }
    };
}
pub(crate) use ensure;
