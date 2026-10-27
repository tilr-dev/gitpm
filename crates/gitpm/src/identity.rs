//! Stable UUID package IDs, human aliases and repository release tags.
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
        "package names must use 1–80 ASCII letters, digits, '-' or '_', not starting with '-'"
    );
    Ok(())
}
/// Generate a UUID v4 using the operating system's random source.
pub fn new_id() -> Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
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
    fn generated_ids_are_canonical_v4() {
        let id = new_id().unwrap();
        crate::valid_id(&id).unwrap();
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn release_tag_is_repository_version() {
        assert_eq!(release_tag("1.2.3"), "v1.2.3");
        assert_eq!(release_tag("2.0.0-beta.1"), "v2.0.0-beta.1");
    }
}
