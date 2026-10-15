//! Namespaces, stable plugin IDs and the namespace config file (path supplied by the caller).
use crate::error::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const DEFAULT_NAMESPACE: &str = "local";

pub fn valid_part(part: &str) -> Result<()> {
    ensure!(
        !part.is_empty()
            && part.len() <= 80
            && part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        InvalidId,
        "namespace/name must use 1–80 ASCII letters, digits, '-' or '_' per component"
    );
    Ok(())
}
pub fn qualified(namespace: &str, name: &str) -> Result<String> {
    valid_part(namespace)?;
    valid_part(name)?;
    Ok(format!("{namespace}/{name}"))
}
pub fn release_tag(id: &str, version: &str) -> String {
    if id.contains('/') {
        format!("{id}@{version}")
    } else {
        format!("v{version}")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    namespace: String,
}

/// Namespace stored in a `.pm.json`-style config file; `None` if the file does not exist.
pub fn read_namespace(config: &Path) -> Result<Option<String>> {
    if !config.exists() {
        return Ok(None);
    }
    let value = serde_json::from_slice::<Config>(&fs::read(config)?)
        .map_err(|e| Error::InvalidData(format!("invalid .pm.json: {e}")))?
        .namespace;
    valid_part(&value)?;
    Ok(Some(value))
}

/// Resolve the effective namespace: explicit value, else config file, else `local`.
pub fn resolve_namespace(explicit: Option<&str>, config: &Path) -> Result<String> {
    let value = match explicit {
        Some(value) => value.to_owned(),
        None => read_namespace(config)?.unwrap_or_else(|| DEFAULT_NAMESPACE.into()),
    };
    valid_part(&value)?;
    Ok(value)
}

/// Atomically write the namespace config file.
pub fn write_namespace(config: &Path, namespace: &str) -> Result<()> {
    valid_part(namespace)?;
    let temp = config.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(
        &temp,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Config {
                namespace: namespace.into()
            })?
        ),
    )?;
    fs::rename(temp, config)?;
    Ok(())
}
