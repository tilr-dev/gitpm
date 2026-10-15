//! Release tags are the shared source of truth for installs and catalog generation.
use crate::catalog::version_key;
use crate::error::{Error, Result, ensure};
use crate::{Manifest, exec::git, model::resolve_ref, valid_id};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug)]
pub struct Tag {
    pub name: String,
    pub commit: String,
    pub manifest: Manifest,
}

pub fn discover(repo: &Path, expected_id: Option<&str>) -> Result<Vec<Tag>> {
    let head: Manifest =
        serde_json::from_str(&git(repo, &["show", "HEAD:plugin.json"])?).map_err(|e| {
            Error::InvalidManifest(format!("invalid plugin.json at repository HEAD: {e}"))
        })?;
    let id = expected_id.unwrap_or(&head.id);
    valid_id(id)?;
    let mut releases = BTreeMap::new();
    for name in git(repo, &["tag", "--list"])?.lines() {
        let version = name
            .strip_prefix(&format!("{id}@"))
            .or_else(|| name.strip_prefix('v'));
        let Some(version) = version else { continue };
        let Ok(key) = version_key(version) else {
            continue;
        };
        let reference = format!("refs/tags/{name}");
        let commit = resolve_ref(repo, &reference)?;
        let manifest: Manifest =
            serde_json::from_str(&git(repo, &["show", &format!("{commit}:plugin.json")])?)
                .map_err(|e| {
                    Error::InvalidManifest(format!("invalid manifest in release tag {name}: {e}"))
                })?;
        // Forks retain upstream tags. They are not releases of the fork.
        if manifest.id != id {
            continue;
        }
        ensure!(
            manifest.version == version,
            VersionConflict,
            "tag {name} disagrees with manifest version {}",
            manifest.version
        );
        if let Some(old) = releases.get(&key) {
            let old: &Tag = old;
            ensure!(
                old.commit == commit,
                VersionConflict,
                "conflicting release tags for {id}@{version}"
            );
        } else {
            releases.insert(
                key,
                Tag {
                    name: name.into(),
                    commit,
                    manifest,
                },
            );
        }
    }
    ensure!(
        !releases.is_empty(),
        NoReleases,
        "no release tags for {id}; create vVERSION or {id}@VERSION, or use --channel dev"
    );
    Ok(releases.into_values().collect())
}

pub fn latest(repo: &Path, expected_id: Option<&str>) -> Result<Tag> {
    discover(repo, expected_id)?
        .pop()
        .ok_or_else(|| Error::NoReleases("no releases".into()))
}
