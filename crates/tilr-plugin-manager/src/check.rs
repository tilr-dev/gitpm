//! Read-only comparison of installed state with selected catalog releases.
use crate::error::{Error, Result};
use crate::{
    State,
    catalog::{Catalog, version_key},
};
use serde::Serialize;
use std::{fs, path::Path};

/// Outcome of comparing one installed plugin with the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    NotInCatalog,
    DifferentSource,
    InvalidCatalogData,
    NoVersions,
    UncomparableVersion,
    SameVersionDifferentCommit,
    UpdateAvailable,
    UpToDate,
    NewerThanCatalog,
}
impl CheckStatus {
    /// Human-readable status (Russian, as shown by the stand CLI).
    pub fn message(self) -> &'static str {
        match self {
            CheckStatus::NotInCatalog => "нет в каталоге",
            CheckStatus::DifferentSource => "другой source в каталоге",
            CheckStatus::InvalidCatalogData => "некорректные данные каталога",
            CheckStatus::NoVersions => "нет версий в каталоге",
            CheckStatus::UncomparableVersion => "версию нельзя сравнить",
            CheckStatus::SameVersionDifferentCommit => "тот же номер, другой commit",
            CheckStatus::UpdateAvailable => "доступно обновление",
            CheckStatus::UpToDate => "актуален",
            CheckStatus::NewerThanCatalog => "новее каталога",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckRow {
    pub id: String,
    pub installed: String,
    /// Latest catalog version, `None` if unknown.
    pub latest: Option<String>,
    pub enabled: bool,
    pub update_available: bool,
    pub status: CheckStatus,
}
/// Backwards-compatible name.
pub type Row = CheckRow;

/// Read `state.json` without opening (locking or creating) the workspace.
/// A missing workspace means "nothing installed".
pub fn read_state(root: &Path) -> Result<State> {
    let path = root.join("state.json");
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| Error::InvalidData(format!("invalid workspace state.json: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !root.exists() => {
            Ok(State::default())
        }
        Err(e) => Err(Error::Io(format!("cannot read {}: {e}", path.display()))),
    }
}

pub fn compare(state: &State, catalog: &Catalog) -> Vec<CheckRow> {
    state
        .plugins
        .iter()
        .map(|(id, installed)| {
            let mut row = CheckRow {
                id: id.clone(),
                installed: installed.current.version.clone(),
                latest: None,
                enabled: installed.enabled,
                update_available: false,
                status: CheckStatus::NotInCatalog,
            };
            let Some(plugin) = catalog.plugins.get(id) else {
                return row;
            };
            // Do not suggest replacing a local fork with an unrelated repository of the same ID.
            if plugin.source != installed.source {
                row.status = CheckStatus::DifferentSource;
                return row;
            }
            let versions = plugin
                .versions
                .iter()
                .map(|(version, release)| {
                    let key = version_key(version).ok()?;
                    if release.manifest.id != *id
                        || release.manifest.version != *version
                        || release.source != plugin.source
                    {
                        return None;
                    }
                    Some((key, release))
                })
                .collect::<Option<Vec<_>>>();
            let Some(versions) = versions else {
                row.status = CheckStatus::InvalidCatalogData;
                return row;
            };
            let Some((latest_key, latest)) = versions.into_iter().max_by_key(|(key, _)| *key)
            else {
                row.status = CheckStatus::NoVersions;
                return row;
            };
            row.latest = Some(latest.manifest.version.clone());
            let Ok(current_key) = version_key(&installed.current.version) else {
                row.status = CheckStatus::UncomparableVersion;
                return row;
            };
            if let Some(current) = plugin.versions.get(&installed.current.version)
                && current.commit != installed.current.commit
            {
                row.status = CheckStatus::SameVersionDifferentCommit;
                return row;
            }
            match latest_key.cmp(&current_key) {
                std::cmp::Ordering::Greater => {
                    row.update_available = true;
                    row.status = CheckStatus::UpdateAvailable;
                }
                std::cmp::Ordering::Equal => row.status = CheckStatus::UpToDate,
                std::cmp::Ordering::Less => row.status = CheckStatus::NewerThanCatalog,
            }
            row
        })
        .collect()
}

/// Read the workspace state and compare it with the catalog. An empty or missing
/// workspace does not touch the catalog at all.
pub fn check(root: &Path, catalog_location: &str) -> Result<Vec<CheckRow>> {
    let state = read_state(root)?;
    if state.plugins.is_empty() {
        return Ok(Vec::new());
    }
    let snapshot = crate::catalog::selected(catalog_location, &state)?;
    Ok(compare(&state, &snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Installed, Manifest, Revision,
        catalog::{Plugin, Release},
    };
    pub(crate) fn fixture() -> (State, Catalog) {
        let installed = Installed {
            source: "https://example.com/demo.git".into(),
            current: Revision {
                version: "1.9.0".into(),
                commit: "a".repeat(40),
            },
            previous: None,
            enabled: false,
            ..Default::default()
        };
        let versions = ["1.9.0", "1.10.0"]
            .into_iter()
            .map(|version| {
                (
                    version.into(),
                    Release {
                        manifest: Manifest {
                            id: "demo".into(),
                            version: version.into(),
                            ..Default::default()
                        },
                        source: installed.source.clone(),
                        commit: if version == "1.9.0" {
                            "a".repeat(40)
                        } else {
                            "b".repeat(40)
                        },
                        reference: format!("v{version}"),
                        files: Default::default(),
                        build: None,
                    },
                )
            })
            .collect();
        let catalog = Catalog {
            plugins: [(
                "demo".into(),
                Plugin {
                    source: installed.source.clone(),
                    versions,
                },
            )]
            .into(),
        };
        (
            State {
                plugins: [("demo".into(), installed)].into(),
                ..Default::default()
            },
            catalog,
        )
    }
    #[test]
    fn numeric_latest_includes_disabled_and_does_not_modify_state() {
        let (state, catalog) = fixture();
        let before = serde_json::to_value(&state).unwrap();
        let rows = compare(&state, &catalog);
        assert_eq!(rows[0].latest.as_deref(), Some("1.10.0"));
        assert!(rows[0].update_available && !rows[0].enabled);
        assert_eq!(rows.iter().filter(|r| r.update_available).count(), 1);
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    #[test]
    fn missing_source_and_commit_mismatch_are_not_updates() {
        let (mut state, mut catalog) = fixture();
        assert_eq!(
            compare(&state, &Catalog::default())[0].status.message(),
            "нет в каталоге"
        );
        catalog.plugins.get_mut("demo").unwrap().source = "another".into();
        assert_eq!(
            compare(&state, &catalog)[0].status.message(),
            "другой source в каталоге"
        );
        let (_, catalog) = fixture();
        state.plugins.get_mut("demo").unwrap().current.commit = "c".repeat(40);
        let rows = compare(&state, &catalog);
        assert_eq!(rows[0].status.message(), "тот же номер, другой commit");
        assert!(!rows[0].update_available);
    }
    #[test]
    fn current_and_ahead_versions() {
        let (mut state, catalog) = fixture();
        let plugin = state.plugins.get_mut("demo").unwrap();
        plugin.current.version = "1.10.0".into();
        plugin.current.commit = "b".repeat(40);
        assert_eq!(compare(&state, &catalog)[0].status.message(), "актуален");
        state.plugins.get_mut("demo").unwrap().current.version = "2.0.0".into();
        assert_eq!(
            compare(&state, &catalog)[0].status.message(),
            "новее каталога"
        );
    }
    fn status(state: &State, catalog: &Catalog) -> CheckStatus {
        compare(state, catalog)[0].status
    }

    #[test]
    fn invalid_catalog_entries_are_reported_not_trusted() {
        let (state, catalog) = fixture();
        let mutate: [fn(&mut Release); 4] = [
            |r| r.manifest.id = "other".into(),
            |r| r.manifest.version = "1.10.1".into(),
            |r| r.source = "elsewhere".into(),
            |_| {},
        ];
        for (n, change) in mutate.into_iter().enumerate() {
            let mut catalog = catalog.clone();
            change(
                catalog
                    .plugins
                    .get_mut("demo")
                    .unwrap()
                    .versions
                    .get_mut("1.10.0")
                    .unwrap(),
            );
            let row = &compare(&state, &catalog)[0];
            if n == 3 {
                assert_eq!(row.status, CheckStatus::UpdateAvailable);
            } else {
                assert_eq!(row.status, CheckStatus::InvalidCatalogData, "case {n}");
                assert!(row.latest.is_none() && !row.update_available);
            }
        }
        // A version key that is not MAJOR.MINOR.PATCH invalidates the entry too.
        let mut bad = catalog.clone();
        let plugin = bad.plugins.get_mut("demo").unwrap();
        let mut release = plugin.versions["1.10.0"].clone();
        release.manifest.version = "2.0".into();
        plugin.versions.insert("2.0".into(), release);
        assert_eq!(status(&state, &bad), CheckStatus::InvalidCatalogData);
    }

    #[test]
    fn no_versions_and_uncomparable_installed_version() {
        let (mut state, mut catalog) = fixture();
        catalog.plugins.get_mut("demo").unwrap().versions.clear();
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::NoVersions);
        assert!(row.latest.is_none());

        let (_, catalog) = fixture();
        state.plugins.get_mut("demo").unwrap().current.version = "1.9".into();
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::UncomparableVersion);
        assert_eq!(row.latest.as_deref(), Some("1.10.0"));
        assert!(!row.update_available);
    }

    #[test]
    fn installed_version_missing_from_catalog_still_compares_numerically() {
        let (mut state, catalog) = fixture();
        let current = &mut state.plugins.get_mut("demo").unwrap().current;
        current.version = "1.9.5".into();
        current.commit = "f".repeat(40);
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::UpdateAvailable);
        assert!(row.update_available);
    }

    #[test]
    fn rows_follow_state_order_and_match_full_ids() {
        let (state, catalog) = fixture();
        let mut state = state;
        let demo = state.plugins["demo"].clone();
        // Same name in another namespace is a different plugin.
        state.plugins.insert("dan/demo".into(), demo.clone());
        state.plugins.insert("a/zzz".into(), demo);
        let rows = compare(&state, &catalog);
        let summary: Vec<_> = rows.iter().map(|r| (r.id.as_str(), r.status)).collect();
        assert_eq!(
            summary,
            [
                ("a/zzz", CheckStatus::NotInCatalog),
                ("dan/demo", CheckStatus::NotInCatalog),
                ("demo", CheckStatus::UpdateAvailable),
            ]
        );
        assert_eq!(rows[0].installed, "1.9.0");
        assert!(compare(&State::default(), &catalog).is_empty());
    }

    #[test]
    fn statuses_have_distinct_messages_and_snake_case_names() {
        use std::collections::BTreeSet;
        let all = [
            (CheckStatus::NotInCatalog, "not_in_catalog"),
            (CheckStatus::DifferentSource, "different_source"),
            (CheckStatus::InvalidCatalogData, "invalid_catalog_data"),
            (CheckStatus::NoVersions, "no_versions"),
            (CheckStatus::UncomparableVersion, "uncomparable_version"),
            (
                CheckStatus::SameVersionDifferentCommit,
                "same_version_different_commit",
            ),
            (CheckStatus::UpdateAvailable, "update_available"),
            (CheckStatus::UpToDate, "up_to_date"),
            (CheckStatus::NewerThanCatalog, "newer_than_catalog"),
        ];
        for (status, name) in all {
            assert_eq!(serde_json::to_value(status).unwrap(), name);
            assert!(!status.message().is_empty());
        }
        let messages: BTreeSet<_> = all.iter().map(|(s, _)| s.message()).collect();
        assert_eq!(messages.len(), all.len());
        assert_eq!(
            CheckStatus::UpdateAvailable.message(),
            "доступно обновление"
        );
        let (state, catalog) = fixture();
        let row: Row = compare(&state, &catalog).remove(0);
        let value = serde_json::to_value(row).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "id": "demo", "installed": "1.9.0", "latest": "1.10.0",
                "enabled": false, "update_available": true, "status": "update_available"
            })
        );
    }

    #[test]
    fn read_state_never_creates_or_locks_a_workspace() {
        use crate::test_support::TempDir;
        let dir = TempDir::new("check");
        let root = dir.join("workspace");
        assert!(read_state(&root).unwrap().plugins.is_empty());
        assert!(!root.exists(), "read_state must not create the workspace");
        // An existing directory without state.json is not an empty workspace.
        fs::create_dir(&root).unwrap();
        assert_eq!(read_state(&root).unwrap_err().kind(), "io");
        fs::write(root.join("state.json"), "{truncated").unwrap();
        let error = read_state(&root).unwrap_err();
        assert_eq!(error.kind(), "invalid_data");
        assert!(
            error
                .to_string()
                .starts_with("invalid workspace state.json")
        );
        let (state, _) = fixture();
        fs::write(
            root.join("state.json"),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .unwrap();
        assert_eq!(read_state(&root).unwrap().plugins.len(), 1);
    }

    #[test]
    fn check_skips_the_catalog_for_an_empty_workspace_and_fails_on_a_bad_catalog() {
        use crate::test_support::TempDir;
        let dir = TempDir::new("check");
        let root = dir.join("workspace");
        // The catalog location is invalid, but it is never read.
        assert!(check(&root, "ftp://invalid").unwrap().is_empty());
        fs::create_dir(&root).unwrap();
        fs::write(root.join("state.json"), r#"{"plugins":{}}"#).unwrap();
        assert!(check(&root, "ftp://invalid").unwrap().is_empty());

        let (state, _) = fixture();
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            check(&root, "ftp://invalid").unwrap_err().kind(),
            "invalid_argument"
        );
        // A missing catalog is an error, not "no updates".
        let missing = dir.join("missing-catalog");
        assert_eq!(
            check(&root, missing.to_str().unwrap()).unwrap_err().kind(),
            "catalog"
        );
    }

    #[test]
    fn check_reads_a_local_v2_catalog() {
        use crate::test_support::{TempDir, release};
        let dir = TempDir::new("check");
        let catalog = dir.join("catalog");
        let source = "https://example.com/demo.git";
        for (version, c) in [("1.9.0", 'a'), ("1.10.0", 'b'), ("1.2.0", 'c')] {
            crate::catalog::tests::save(&catalog, &release("dan/demo", version, source, c));
        }
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        let state = crate::test_support::state_with("dan/demo", source, "1.9.0", &"a".repeat(40));
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let rows = check(&root, catalog.to_str().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, CheckStatus::UpdateAvailable);
        assert_eq!(rows[0].latest.as_deref(), Some("1.10.0"));
    }
}
