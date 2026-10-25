//! Namespaces, stable package IDs and release tags. There is no default namespace: every
//! ID is given in full (`namespace/name`).
use crate::error::{Result, ensure};

pub fn valid_part(part: &str) -> Result<()> {
    ensure!(
        !part.is_empty()
            && part.len() <= 80
            && !part.starts_with('-')
            && part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        InvalidId,
        "namespace/name must use 1–80 ASCII letters, digits, '-' or '_' per component, not starting with '-'"
    );
    Ok(())
}
pub fn qualified(namespace: &str, name: &str) -> Result<String> {
    valid_part(namespace)?;
    valid_part(name)?;
    Ok(format!("{namespace}/{name}"))
}
/// Split a full package ID `NAMESPACE/NAME` into its validated parts. A name without a
/// namespace is refused with [`crate::Error::InvalidId`]: there is no default namespace.
pub fn split(id: &str) -> Result<(&str, &str)> {
    let Some((namespace, name)) = id.split_once('/') else {
        crate::error::fail!(
            InvalidId,
            "package ID needs a namespace: use NAMESPACE/NAME, for example acme/{id}"
        );
    };
    qualified(namespace, name)?;
    Ok((namespace, name))
}
/// The repository release tag `vVERSION`, the only tag spelling gitpm creates and reads.
pub fn release_tag(version: &str) -> String {
    format!("v{version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_part_accepts_1_to_80_safe_ascii_characters() {
        for ok in [
            "a",
            "Z",
            "0",
            "_",
            "_-",
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
            "-",
            "-x",
            "--help",
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
    fn split_needs_a_namespace() {
        assert_eq!(split("acme/hello").unwrap(), ("acme", "hello"));
        let error = split("hello").unwrap_err();
        assert_eq!(error.kind(), "invalid_id");
        assert_eq!(
            error.to_string(),
            "package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello"
        );
        assert_eq!(split("a/b/c").unwrap_err().kind(), "invalid_id");
    }

    #[test]
    fn release_tag_is_repository_version() {
        assert_eq!(release_tag("1.2.3"), "v1.2.3");
        assert_eq!(release_tag("2.0.0-beta.1"), "v2.0.0-beta.1");
    }
}
