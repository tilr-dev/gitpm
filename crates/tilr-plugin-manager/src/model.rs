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
