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
            fork_of: None,
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
                            dependencies: Default::default(),
                            fork_of: None,
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
}
