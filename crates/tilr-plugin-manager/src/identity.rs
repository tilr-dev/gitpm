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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn valid_part_accepts_1_to_80_safe_ascii_characters() {
        for ok in [
            "a",
            "Z",
            "0",
            "-",
            "_",
            "a-b",
            "a_b",
            "Demo1",
            &"x".repeat(80),
        ] {
            assert!(valid_part(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            &"x".repeat(81),
            ".",
            "..",
            "a/b",
            "a\\b",
            "a b",
            "a.b",
            "é",
            "a\0",
            "a\n",
            "~",
        ] {
            let error = valid_part(bad).unwrap_err();
            assert_eq!(error.kind(), "invalid_id", "{bad:?}");
        }
    }

    #[test]
    fn qualified_validates_both_parts() {
        assert_eq!(qualified("dan", "demo1").unwrap(), "dan/demo1");
        assert_eq!(qualified("", "demo1").unwrap_err().kind(), "invalid_id");
        assert_eq!(qualified("dan", "..").unwrap_err().kind(), "invalid_id");
        assert_eq!(qualified("dan", "a/b").unwrap_err().kind(), "invalid_id");
    }

    #[test]
    fn release_tag_is_v_for_legacy_and_at_for_scoped_ids() {
        assert_eq!(release_tag("demo1", "1.2.3"), "v1.2.3");
        assert_eq!(release_tag("dan/demo1", "1.2.3"), "dan/demo1@1.2.3");
    }

    #[test]
    fn namespace_config_round_trip_and_format() {
        let dir = TempDir::new("identity");
        let config = dir.join(".pm.json");
        assert_eq!(read_namespace(&config).unwrap(), None);
        assert_eq!(resolve_namespace(None, &config).unwrap(), DEFAULT_NAMESPACE);
        write_namespace(&config, "dan").unwrap();
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "{\n  \"namespace\": \"dan\"\n}\n"
        );
        assert_eq!(read_namespace(&config).unwrap().as_deref(), Some("dan"));
        assert_eq!(resolve_namespace(None, &config).unwrap(), "dan");
        // Explicit value wins over the file.
        assert_eq!(resolve_namespace(Some("other"), &config).unwrap(), "other");
        // Only the config file remains (no temporary files).
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [".pm.json"]);
    }

    #[test]
    fn invalid_namespace_values_are_rejected() {
        let dir = TempDir::new("identity");
        let config = dir.join(".pm.json");
        assert_eq!(
            resolve_namespace(Some("../x"), &config).unwrap_err().kind(),
            "invalid_id"
        );
        assert_eq!(
            write_namespace(&config, "a/b").unwrap_err().kind(),
            "invalid_id"
        );
        assert!(!config.exists(), "invalid namespace must not be written");

        fs::write(&config, "{\"namespace\":\"a b\"}").unwrap();
        assert_eq!(read_namespace(&config).unwrap_err().kind(), "invalid_id");
        fs::write(&config, "{\"namespace\":\"dan\",\"extra\":1}").unwrap();
        assert_eq!(read_namespace(&config).unwrap_err().kind(), "invalid_data");
        fs::write(&config, "{broken").unwrap();
        let error = resolve_namespace(None, &config).unwrap_err();
        assert_eq!(error.kind(), "invalid_data");
        assert!(error.to_string().starts_with("invalid .pm.json"));
        // An explicit namespace does not read a broken file.
        assert_eq!(resolve_namespace(Some("dan"), &config).unwrap(), "dan");
    }
}
