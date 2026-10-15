//! Persistent formats: plugin manifest and workspace state. Byte-compatible with the stand.
use crate::error::{Error, Result, ensure};
use crate::{exec::git, identity};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForkOf {
    pub id: String,
    pub version: String,
    pub commit: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Revision {
    pub version: String,
    pub commit: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Installed {
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    #[serde(default, rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct State {
    pub plugins: BTreeMap<String, Installed>,
}

/// Flat view of one installed plugin (convenient for UIs and FFI).
#[derive(Clone, Debug, Serialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    #[serde(rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
}
impl State {
    pub fn installed(&self) -> Vec<InstalledPlugin> {
        self.plugins
            .iter()
            .map(|(id, p)| InstalledPlugin {
                id: id.clone(),
                source: p.source.clone(),
                current: p.current.clone(),
                previous: p.previous.clone(),
                enabled: p.enabled,
                fork_of: p.fork_of.clone(),
            })
            .collect()
    }
}

/// `namespace/name` or a legacy unscoped ID; every component is a safe path part.
pub fn valid_id(id: &str) -> Result<()> {
    let parts: Vec<_> = id.split('/').collect();
    ensure!(
        (1..=2).contains(&parts.len()),
        InvalidId,
        "expected namespace/name (or a legacy unscoped ID)"
    );
    for part in parts {
        identity::valid_part(part)?;
    }
    Ok(())
}

pub(crate) fn manifest(repo: &Path) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(&fs::read(repo.join("plugin.json"))?)
        .map_err(|e| Error::InvalidManifest(format!("invalid plugin.json: {e}")))?;
    valid_id(&m.id)?;
    if let Some(base) = &m.fork_of {
        valid_id(&base.id)?;
        ensure!(
            base.id != m.id,
            InvalidManifest,
            "fork must have its own ID"
        );
        ensure!(
            !base.version.trim().is_empty(),
            InvalidManifest,
            "forkOf.version is empty"
        );
        ensure!(
            matches!(base.commit.len(), 40 | 64)
                && base.commit.bytes().all(|c| c.is_ascii_hexdigit()),
            InvalidManifest,
            "invalid forkOf.commit"
        );
    }
    ensure!(
        !m.version.trim().is_empty(),
        InvalidManifest,
        "version must not be empty"
    );
    crate::catalog::version_key(&m.version)?;
    if let Some(base) = &m.fork_of {
        crate::catalog::version_key(&base.version)?;
    }
    ensure!(
        m.dependencies.is_empty(),
        Unsupported,
        "dependencies are not supported in this first prototype"
    );
    ensure!(
        repo.join("dist").is_dir(),
        InvalidManifest,
        "plugin must contain a ready dist/ directory"
    );
    Ok(m)
}

pub(crate) fn revision(repo: &Path, m: &Manifest) -> Result<Revision> {
    Ok(Revision {
        version: m.version.clone(),
        commit: git(repo, &["rev-parse", "HEAD"])?,
    })
}

pub(crate) fn resolve_ref(repo: &Path, reference: &str) -> Result<String> {
    ensure!(
        !reference.is_empty() && !reference.starts_with('-'),
        InvalidArgument,
        "invalid git ref"
    );
    git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )
}

pub(crate) fn manifest_json(m: &Manifest) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(m)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir, git as fixture_git};
    use std::path::PathBuf;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn valid_id_accepts_legacy_and_scoped_ids_only() {
        for ok in [
            "demo1",
            "dan/demo1",
            "a-b/c_d",
            &format!("{}/{}", "x".repeat(80), "y".repeat(80)),
        ] {
            assert!(valid_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "/",
            "dan/",
            "/demo1",
            "a/b/c",
            "../demo1",
            "dan/..",
            "dan/.",
            "dan\\demo1",
            "dan/demo 1",
            "Dan/démo",
            &"x".repeat(81),
        ] {
            assert_eq!(valid_id(bad).unwrap_err().kind(), "invalid_id", "{bad:?}");
        }
    }

    fn repo_with(dir: &TempDir, json: &str, dist: bool) -> PathBuf {
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("plugin.json"), json).unwrap();
        if dist {
            fs::create_dir_all(repo.join("dist")).unwrap();
        }
        repo
    }

    #[test]
    fn manifest_parses_minimal_and_fork_manifests() {
        let dir = TempDir::new("model");
        let repo = repo_with(&dir, r#"{"id":"dan/demo1","version":"0.1.0"}"#, true);
        let m = manifest(&repo).unwrap();
        assert_eq!((m.id.as_str(), m.version.as_str()), ("dan/demo1", "0.1.0"));
        assert!(m.dependencies.is_empty() && m.fork_of.is_none());

        let json = format!(
            r#"{{"id":"me/demo1","version":"0.2.0","dependencies":{{}},"forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}"}}}}"#
        );
        fs::write(repo.join("plugin.json"), json).unwrap();
        let m = manifest(&repo).unwrap();
        assert_eq!(
            m.fork_of,
            Some(ForkOf {
                id: "dan/demo1".into(),
                version: "0.1.0".into(),
                commit: COMMIT.into()
            })
        );
        // SHA-256 object IDs are accepted as fork commits too.
        let json = format!(
            r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{}"}}}}"#,
            "a".repeat(64)
        );
        fs::write(repo.join("plugin.json"), json).unwrap();
        assert!(manifest(&repo).is_ok());
    }

    #[test]
    fn manifest_rejections_have_precise_kinds() {
        let dir = TempDir::new("model");
        let fork = |id: &str, version: &str, commit: &str| {
            format!(
                r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"{id}","version":"{version}","commit":"{commit}"}}}}"#
            )
        };
        let cases: Vec<(String, bool, &str)> = vec![
            ("{".into(), true, "invalid_manifest"),
            (r#"{"id":"demo1"}"#.into(), true, "invalid_manifest"),
            (
                r#"{"id":"demo1","version":"0.1.0","extra":true}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"demo1","version":"0.1.0","pinned":true}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"../demo1","version":"0.1.0"}"#.into(),
                true,
                "invalid_id",
            ),
            (
                r#"{"id":"a/b/c","version":"0.1.0"}"#.into(),
                true,
                "invalid_id",
            ),
            (
                r#"{"id":"demo1","version":"  "}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"demo1","version":"0.1.0"}"#.into(),
                false,
                "invalid_manifest",
            ),
            (
                r#"{"id":"demo1","version":"0.1.0","dependencies":{"dan/base":"^1.0.0"}}"#.into(),
                true,
                "unsupported",
            ),
            (fork("me/demo1", "0.1.0", COMMIT), true, "invalid_manifest"),
            (fork("dan/demo1", " ", COMMIT), true, "invalid_manifest"),
            (
                fork("dan/demo1", "0.1.0", &COMMIT[..39]),
                true,
                "invalid_manifest",
            ),
            (
                fork("dan/demo1", "0.1.0", &"g".repeat(40)),
                true,
                "invalid_manifest",
            ),
            (fork("../x", "0.1.0", COMMIT), true, "invalid_id"),
            (
                format!(
                    r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}","extra":1}}}}"#
                ),
                true,
                "invalid_manifest",
            ),
        ];
        for (json, dist, kind) in cases {
            let _ = fs::remove_dir_all(dir.join("repo"));
            let repo = repo_with(&dir, &json, dist);
            let error = manifest(&repo).unwrap_err();
            assert_eq!(error.kind(), kind, "{json}: {error}");
        }
        // A missing plugin.json is an I/O error.
        let empty = dir.join("empty");
        fs::create_dir_all(empty.join("dist")).unwrap();
        assert_eq!(manifest(&empty).unwrap_err().kind(), "io");
    }

    #[test]
    fn manifest_rejects_non_numeric_versions() {
        let dir = TempDir::new("model");
        for version in ["1.0", "1.0.0-beta", "1.0.0+build", "banana", "01.0.0"] {
            let _ = fs::remove_dir_all(dir.join("repo"));
            let repo = repo_with(
                &dir,
                &format!(r#"{{"id":"demo1","version":"{version}"}}"#),
                true,
            );
            assert!(manifest(&repo).is_err(), "{version} must be rejected");
        }
    }

    #[test]
    fn manifest_json_is_pretty_with_trailing_newline_and_stable_fields() {
        let mut m = Manifest {
            id: "dan/demo1".into(),
            version: "0.1.0".into(),
            dependencies: Default::default(),
            fork_of: None,
        };
        assert_eq!(
            manifest_json(&m).unwrap(),
            "{\n  \"id\": \"dan/demo1\",\n  \"version\": \"0.1.0\",\n  \"dependencies\": {}\n}\n"
        );
        m.fork_of = Some(ForkOf {
            id: "vendor/demo1".into(),
            version: "0.0.9".into(),
            commit: COMMIT.into(),
        });
        let text = manifest_json(&m).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["forkOf"]["id"], "vendor/demo1");
        assert!(text.contains("\"forkOf\""), "camelCase key: {text}");
        let back: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.fork_of, m.fork_of);
    }

    /// Byte format written by the stand prototype (`serde_json::to_vec_pretty`, no newline).
    const STATE: &str = r#"{
  "plugins": {
    "dan/demo1": {
      "source": "git@github.com:dan/demo1.git",
      "current": {
        "version": "0.1.1",
        "commit": "e23af971029e7c8c4d7d069e20e543e3c2ee63ad"
      },
      "previous": {
        "version": "0.1.0",
        "commit": "a17e286b69daa620096f78fa54153696b8459b6b"
      },
      "enabled": false
    },
    "demo1": {
      "source": "/tmp/demo1",
      "current": {
        "version": "1.0.0",
        "commit": "a6e1cb27e482634979ffcfb4f9cdd02ff99005db"
      },
      "previous": null,
      "enabled": true,
      "forkOf": {
        "id": "vendor/demo1",
        "version": "0.9.0",
        "commit": "0123456789abcdef0123456789abcdef01234567"
      }
    }
  }
}"#;

    #[test]
    fn state_round_trips_byte_for_byte() {
        let state: State = serde_json::from_str(STATE).unwrap();
        assert_eq!(
            String::from_utf8(serde_json::to_vec_pretty(&state).unwrap()).unwrap(),
            STATE
        );
        let p = &state.plugins["dan/demo1"];
        assert!(!p.enabled && p.fork_of.is_none());
        assert_eq!(p.previous.as_ref().unwrap().version, "0.1.0");
        assert_eq!(
            state.plugins["demo1"].fork_of.as_ref().unwrap().id,
            "vendor/demo1"
        );
        // Default state is the file a new workspace starts with.
        assert_eq!(
            serde_json::to_string_pretty(&State::default()).unwrap(),
            "{\n  \"plugins\": {}\n}"
        );
    }

    #[test]
    fn state_requires_core_fields() {
        for broken in [
            r#"{}"#,
            r#"{"plugins":{"a":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null}}}"#,
            r#"{"plugins":{"a":{"current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true}}}"#,
            r#"{"plugins":{"a":{"source":"s","current":{"version":"1.0.0"},"previous":null,"enabled":true}}}"#,
        ] {
            assert!(serde_json::from_str::<State>(broken).is_err(), "{broken}");
        }
        // forkOf is optional; previous may be null.
        let ok = r#"{"plugins":{"a":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true}}}"#;
        assert!(serde_json::from_str::<State>(ok).is_ok());
    }

    #[test]
    fn installed_is_a_sorted_flat_view() {
        let state: State = serde_json::from_str(STATE).unwrap();
        let list = state.installed();
        assert_eq!(
            list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["dan/demo1", "demo1"]
        );
        assert_eq!(list[0].source, "git@github.com:dan/demo1.git");
        assert_eq!(list[0].current.version, "0.1.1");
        assert_eq!(
            list[0].previous.as_ref().unwrap().commit,
            "a17e286b69daa620096f78fa54153696b8459b6b"
        );
        assert!(!list[0].enabled);
        assert_eq!(list[1].fork_of.as_ref().unwrap().version, "0.9.0");
        let value = serde_json::to_value(&list[1]).unwrap();
        assert_eq!(value["forkOf"]["id"], "vendor/demo1");
        assert!(
            serde_json::to_value(&list[0])
                .unwrap()
                .get("forkOf")
                .is_none()
        );
    }

    #[test]
    fn resolve_ref_refuses_option_like_refs_and_resolves_commits() {
        for bad in ["", "-x", "--upload-pack=evil"] {
            // Rejected before git runs: the path does not even exist.
            let error = resolve_ref(Path::new("/nonexistent-pm-repo"), bad).unwrap_err();
            assert_eq!(error.kind(), "invalid_argument", "{bad:?}");
        }
        let dir = TempDir::new("model");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let first = author.head();
        let second = author.commit("0.2.0");
        assert_eq!(resolve_ref(&author.path, "dan/demo1@0.1.0").unwrap(), first);
        assert_eq!(
            resolve_ref(&author.path, "refs/tags/dan/demo1@0.1.0").unwrap(),
            first
        );
        assert_eq!(resolve_ref(&author.path, "main").unwrap(), second);
        assert_eq!(resolve_ref(&author.path, &first[..12]).unwrap(), first);
        assert_eq!(
            resolve_ref(&author.path, "missing").unwrap_err().kind(),
            "git"
        );
        // A tree object is not a commit.
        let tree = fixture_git(&author.path, &["rev-parse", "HEAD^{tree}"]);
        assert_eq!(resolve_ref(&author.path, &tree).unwrap_err().kind(), "git");
        let m = manifest(&author.path).unwrap();
        assert_eq!(
            revision(&author.path, &m).unwrap(),
            Revision {
                version: "0.2.0".into(),
                commit: second
            }
        );
    }
}
