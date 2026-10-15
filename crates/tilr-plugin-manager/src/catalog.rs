//! Catalog client and author operations. The catalog stores metadata, never code.
use crate::error::{Context, Error, Require, Result, ensure};
use crate::{Manifest, exec, exec::git, identity, model::manifest, model::resolve_ref, valid_id};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub manifest: Manifest,
    pub source: String,
    pub commit: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, FileDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<Build>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub source: String,
    pub versions: BTreeMap<String, Release>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub plugins: BTreeMap<String, Plugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileDigest {
    pub sha256: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub url: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory {
    schema: u32,
    directories: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginIndex {
    schema: u32,
    id: String,
    source: String,
    versions: BTreeMap<String, String>,
}
fn folder(id: &str) -> Result<String> {
    valid_id(id)?;
    // Separate legacy IDs so a plugin cannot collide with a namespace.
    Ok(if id.contains('/') {
        format!("namespaces/{id}")
    } else {
        format!("legacy/{id}")
    })
}
pub fn hash_files(dist: &Path) -> Result<BTreeMap<String, FileDigest>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    fn walk(base: &Path, path: &Path, result: &mut BTreeMap<String, FileDigest>) -> Result<()> {
        let meta = fs::symlink_metadata(path)?;
        ensure!(
            !meta.file_type().is_symlink(),
            Unsupported,
            "build contains a symlink"
        );
        if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(base, &entry?.path(), result)?;
            }
        } else {
            ensure!(meta.is_file(), Unsupported, "unsupported build file");
            let mut file = fs::File::open(path)?;
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 65536];
            let mut size = 0;
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                size += n as u64;
            }
            let name = path
                .strip_prefix(base)?
                .to_str()
                .require(Error::InvalidData, "non-UTF8 build path")?
                .replace('\\', "/");
            result.insert(
                name,
                FileDigest {
                    sha256: format!("{:x}", hasher.finalize()),
                    size,
                },
            );
        }
        Ok(())
    }
    ensure!(dist.is_dir(), InvalidManifest, "missing dist directory");
    let mut result = BTreeMap::new();
    walk(dist, dist, &mut result)?;
    Ok(result)
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_release(release: &Release, id: &str, version: &str) -> Result<()> {
    valid_id(id)?;
    version_key(version)?;
    ensure!(
        release.manifest.id == id && release.manifest.version == version,
        Catalog,
        "catalog identity mismatch"
    );
    ensure!(
        matches!(release.commit.len(), 40 | 64)
            && release.commit.bytes().all(|c| c.is_ascii_hexdigit()),
        Catalog,
        "invalid catalog commit"
    );
    for (name, file) in &release.files {
        ensure!(
            !name.is_empty()
                && !name.contains('\\')
                && name
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
                && !name.starts_with('/'),
            Catalog,
            "invalid file path"
        );
        ensure!(valid_hash(&file.sha256), Catalog, "invalid file SHA-256");
    }
    if let Some(build) = &release.build {
        ensure!(
            build.url.starts_with("https://")
                && build.url.len() > 8
                && !build.url.chars().any(char::is_whitespace),
            Catalog,
            "build URL must use HTTPS"
        );
        ensure!(valid_hash(&build.sha256), Catalog, "invalid build SHA-256");
    }
    Ok(())
}

pub fn version_key(version: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<_> = version.split('.').collect();
    ensure!(
        parts.len() == 3
            && parts.iter().all(|p| !p.is_empty()
                && p.bytes().all(|c| c.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))),
        InvalidVersion,
        "version must be MAJOR.MINOR.PATCH without leading zeros"
    );
    Ok((parts[0].parse()?, parts[1].parse()?, parts[2].parse()?))
}

/// True if `location` is a local folder or JSON file (not HTTP).
pub fn is_local(location: &str) -> Result<bool> {
    Ok(local_file(location)?.is_some())
}
fn local_file(location: &str) -> Result<Option<PathBuf>> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(None);
    }
    ensure!(
        !location.contains("://"),
        InvalidArgument,
        "use a filesystem path or HTTP(S) URL"
    );
    let path = PathBuf::from(location);
    Ok(Some(if path.extension().is_some_and(|e| e == "json") {
        path
    } else {
        path.join("catalog.json")
    }))
}

fn read_json(location: &str, relative: &str) -> Result<Option<serde_json::Value>> {
    let bytes = if let Some(path) = local_file(location)? {
        let path = if relative == "catalog.json" {
            path.clone()
        } else {
            path.parent().unwrap().join(relative)
        };
        match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(Error::Io(format!("cannot read {}: {e}", path.display())));
            }
        }
    } else {
        let base = location.trim_end_matches('/');
        let base = if base.ends_with(".json") {
            base.rsplit_once('/').require(Error::Http, "invalid URL")?.0
        } else {
            base
        };
        let url = if relative == "catalog.json" && location.ends_with(".json") {
            location.to_owned()
        } else {
            format!("{base}/{relative}")
        };
        match exec::http_get(&url)? {
            Some(bytes) => bytes,
            None => return Ok(None),
        }
    };
    Ok(Some(
        serde_json::from_slice(&bytes).context(Error::Catalog, "invalid catalog JSON")?,
    ))
}
/// Raw root `catalog.json` at this catalog level.
pub fn list(location: &str) -> Result<serde_json::Value> {
    read_json(location, "catalog.json")?.require(Error::Catalog, "catalog not found")
}
fn plugin_index(location: &str, id: &str) -> Result<Option<PluginIndex>> {
    let Some(value) = read_json(location, &format!("{}/plugin.json", folder(id)?))? else {
        return Ok(None);
    };
    let index: PluginIndex = serde_json::from_value(value)?;
    ensure!(
        index.schema == 2 && index.id == id,
        Catalog,
        "invalid plugin index"
    );
    for (version, link) in &index.versions {
        version_key(version)?;
        ensure!(
            *link == format!("versions/{version}/release.json"),
            Catalog,
            "invalid release link"
        );
    }
    Ok(Some(index))
}
/// Catalog entries relevant to the installed plugins: latest and currently installed versions.
pub fn selected(location: &str, state: &crate::State) -> Result<Catalog> {
    let mut result = Catalog::default();
    for (id, installed) in &state.plugins {
        if let Some(index) = plugin_index(location, id)? {
            let mut versions = BTreeMap::new();
            let latest = index
                .versions
                .keys()
                .max_by_key(|v| version_key(v).unwrap());
            for version in latest.into_iter().chain(
                index
                    .versions
                    .get_key_value(&installed.current.version)
                    .map(|(v, _)| v),
            ) {
                if !versions.contains_key(version) {
                    versions.insert(
                        version.clone(),
                        read_release(location, id, version, &index)?,
                    );
                }
            }
            result.plugins.insert(
                id.clone(),
                Plugin {
                    source: index.source,
                    versions,
                },
            );
        } else {
            let value = list(location)?;
            if value.get("plugins").is_some() {
                let old: Catalog = serde_json::from_value(value)?;
                if let Some(plugin) = old.plugins.get(id) {
                    result.plugins.insert(id.clone(), plugin.clone());
                }
            }
        }
    }
    Ok(result)
}
fn read_release(location: &str, id: &str, version: &str, index: &PluginIndex) -> Result<Release> {
    let link = index
        .versions
        .get(version)
        .require(Error::Catalog, "version not found in catalog")?;
    let release: Release = serde_json::from_value(
        read_json(location, &format!("{}/{link}", folder(id)?))?
            .require(Error::Catalog, "release not found")?,
    )?;
    validate_release(&release, id, version)?;
    ensure!(
        release.source == index.source,
        Catalog,
        "release source mismatch"
    );
    Ok(release)
}

/// A catalog directory an external uploader may publish; returns its canonical path.
pub fn ensure_publishable(location: &str) -> Result<PathBuf> {
    ensure!(
        local_file(location)?.is_some() && Path::new(location).is_dir(),
        InvalidArgument,
        "sync requires a local directory containing catalog.json"
    );
    Ok(fs::canonicalize(location)?)
}

struct CatalogLock(PathBuf);
impl Drop for CatalogLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension("json.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(format!("{}\n", serde_json::to_string_pretty(value)?).as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn lock(path: &Path) -> Result<CatalogLock> {
    fs::create_dir_all(path.parent().unwrap())?;
    let lock_path = path.with_extension("json.lock");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .context(Error::Locked, "catalog locked")?;
    Ok(CatalogLock(lock_path))
}
fn link_directory(path: &Path, name: &str, link: &str) -> Result<()> {
    let mut directory: Directory = if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        Directory {
            schema: 2,
            ..Default::default()
        }
    };
    ensure!(directory.schema == 2, Catalog, "unsupported catalog schema");
    if directory.directories.get(name).is_some_and(|s| s == link) {
        return Ok(());
    }
    ensure!(
        !directory.directories.contains_key(name),
        Catalog,
        "directory link conflict"
    );
    directory.directories.insert(name.into(), link.into());
    atomic_json(path, &directory)
}
fn write_release(root: &Path, release: &Release) -> Result<()> {
    let id = &release.manifest.id;
    let version = &release.manifest.version;
    validate_release(release, id, version)?;
    let folder = folder(id)?;
    let path = root.join(&folder).join("plugin.json");
    let mut index: PluginIndex = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        PluginIndex {
            schema: 2,
            id: id.clone(),
            source: release.source.clone(),
            versions: BTreeMap::new(),
        }
    };
    ensure!(
        index.schema == 2 && index.id == *id && index.source == release.source,
        Catalog,
        "plugin is registered from a different source or identity"
    );
    let link = format!("versions/{version}/release.json");
    let release_path = root.join(&folder).join(&link);
    if release_path.exists() {
        let old: Release = serde_json::from_slice(&fs::read(&release_path)?)?;
        let mut candidate = release.clone();
        // Alternate Git refs can resolve to the same immutable commit. Legacy
        // releases without file digests remain byte-for-byte unchanged on retry.
        candidate.reference = old.reference.clone();
        if old.files.is_empty() {
            candidate.files.clear();
        }
        ensure!(
            serde_json::to_value(&old)? == serde_json::to_value(&candidate)?,
            VersionConflict,
            "version already exists and is immutable"
        );
    } else {
        atomic_json(&release_path, release)?;
    }
    if !index.versions.contains_key(version) {
        index.versions.insert(version.clone(), link);
        atomic_json(&path, &index)?;
    }
    // Publish directory links bottom-up, only when a child first appears.
    let parts: Vec<_> = folder.split('/').collect();
    for depth in (1..parts.len()).rev() {
        let parent = root.join(parts[..depth].join("/")).join("catalog.json");
        link_directory(&parent, parts[depth], &format!("{}/", parts[depth]))?;
    }
    Ok(())
}
fn prepare_root(path: &Path) -> Result<()> {
    if !path.exists() {
        return atomic_json(
            path,
            &Directory {
                schema: 2,
                ..Default::default()
            },
        );
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?).context(
        Error::Catalog,
        "invalid existing catalog; refusing overwrite",
    )?;
    if value.get("plugins").is_some() {
        let old: Catalog = serde_json::from_value(value)?;
        let root = path.parent().unwrap();
        for (id, plugin) in &old.plugins {
            for (version, release) in &plugin.versions {
                validate_release(release, id, version)?;
                ensure!(
                    release.source == plugin.source,
                    Catalog,
                    "legacy source mismatch"
                );
            }
        }
        for plugin in old.plugins.values() {
            for release in plugin.versions.values() {
                write_release(root, release)?;
            }
        }
        let mut directory = Directory {
            schema: 2,
            ..Default::default()
        };
        for id in old.plugins.keys() {
            let folder = folder(id)?;
            let first = folder.split('/').next().unwrap();
            directory
                .directories
                .insert(first.into(), format!("{first}/"));
        }
        let backup = root.join(".catalog-v1.json");
        if !backup.exists() {
            fs::copy(path, backup)?;
        }
        atomic_json(path, &directory)?;
    } else {
        let directory: Directory = serde_json::from_value(value)?;
        ensure!(directory.schema == 2, Catalog, "unsupported catalog schema");
    }
    Ok(())
}
/// Split a local v1 catalog into plugin folders (keeps a `.catalog-v1.json` backup).
pub fn migrate(location: &str) -> Result<()> {
    let path = local_file(location)?
        .require(Error::InvalidArgument, "migration requires local catalog")?;
    let _lock = lock(&path)?;
    prepare_root(&path)
}
fn save_release(path: &Path, release: &Release) -> Result<Release> {
    let _lock = lock(path)?;
    prepare_root(path)?;
    write_release(path.parent().unwrap(), release)?;
    let folder = folder(&release.manifest.id)?;
    let first = folder.split('/').next().unwrap();
    link_directory(path, first, &format!("{first}/"))?;
    let stored = path.parent().unwrap().join(folder).join(format!(
        "versions/{}/release.json",
        release.manifest.version
    ));
    Ok(serde_json::from_slice(&fs::read(stored)?)?)
}
/// Exact release of `id` (the latest one if `version` is `None`).
pub fn lookup(location: &str, id: &str, version: Option<&str>) -> Result<Release> {
    valid_id(id)?;
    if let Some(version) = version {
        version_key(version)?;
    }
    if let Some(index) = plugin_index(location, id)? {
        let version = match version {
            Some(v) => v,
            None => index
                .versions
                .keys()
                .max_by_key(|v| version_key(v).unwrap())
                .require(Error::Catalog, "plugin has no versions")?,
        };
        return read_release(location, id, version, &index);
    }
    let catalog: Catalog = serde_json::from_value(list(location)?)
        .context(Error::Catalog, "plugin not found in catalog")?;
    let plugin = catalog
        .plugins
        .get(id)
        .require(Error::Catalog, "plugin not found in catalog")?;
    let version = match version {
        Some(v) => v.to_owned(),
        None => {
            plugin
                .versions
                .keys()
                .map(|v| Ok((version_key(v)?, v.clone())))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .max()
                .require(Error::Catalog, "plugin has no versions")?
                .1
        }
    };
    let release = plugin
        .versions
        .get(&version)
        .require(Error::Catalog, "version not found in catalog")?
        .clone();
    validate_release(&release, id, &version)?;
    Ok(release)
}
struct Scratch(PathBuf);
impl Scratch {
    /// Unique directory under the OS temporary directory.
    fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
/// Read the exact committed manifest, rather than an author's uncommitted working tree.
pub fn register(url: &str, source: &str, reference: &str) -> Result<Release> {
    register_build(url, source, reference, None)
}
pub fn register_build(
    url: &str,
    source: &str,
    reference: &str,
    build: Option<Build>,
) -> Result<Release> {
    let catalog_path = local_file(url)?.require(
        Error::InvalidArgument,
        "register writes only to a local catalog; prepare locally, then sync files to R2",
    )?;
    ensure!(
        !source.starts_with('-'),
        InvalidArgument,
        "invalid git source"
    );
    let source = if Path::new(source).exists() {
        fs::canonicalize(source)?.to_string_lossy().into_owned()
    } else {
        source.to_owned()
    };
    let temp = Scratch::new("pm-register")?;
    git(&temp.0, &["clone", "--no-hardlinks", "--", &source, "repo"])?;
    let repo = temp.0.join("repo");
    let commit = resolve_ref(&repo, reference)?;
    git(&repo, &["checkout", "--detach", &commit])?;
    let manifest = manifest(&repo)?;
    version_key(&manifest.version)?;
    let release = Release {
        manifest,
        source,
        commit,
        reference: reference.to_owned(),
        files: hash_files(&repo.join("dist"))?,
        build,
    };
    save_release(&catalog_path, &release)
}

/// Progress events of long catalog operations (the library never prints).
#[derive(Debug, Clone, Serialize)]
pub enum Progress {
    /// Cloning a source to discover its release tags.
    ReadingReleases { source: String },
}

/// Result of [`update_sources`].
#[derive(Debug, Clone, Serialize)]
pub struct UpdateSourcesReport {
    /// Number of release tags checked (new or already published).
    pub releases: usize,
    pub sources: Vec<String>,
}

/// Read-only source discovery and hashing precede any catalog changes.
/// Removal from the source list never deletes already published releases.
pub fn update_sources(
    location: &str,
    sources_file: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<UpdateSourcesReport> {
    let path = local_file(location)?.require(
        Error::InvalidArgument,
        "catalog-update requires a local output catalog",
    )?;
    let text = fs::read_to_string(sources_file).context(Error::Io, "cannot read sources file")?;
    let base = sources_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut sources = std::collections::BTreeSet::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        ensure!(
            !line.starts_with('-'),
            InvalidArgument,
            "invalid Git source: {line}"
        );
        let local = base.join(line);
        let source = if local.exists() {
            fs::canonicalize(local)?.to_string_lossy().into_owned()
        } else {
            ensure!(
                line.contains("://") || line.contains('@') && line.contains(':'),
                InvalidArgument,
                "local source does not exist: {line}"
            );
            line.to_owned()
        };
        sources.insert(source);
    }
    ensure!(
        !sources.is_empty(),
        InvalidArgument,
        "sources file has no repositories"
    );
    let temp = Scratch::new("pm-generate")?;
    let mut pending = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for (number, source) in sources.iter().enumerate() {
        progress(Progress::ReadingReleases {
            source: source.clone(),
        });
        let repo = temp.0.join(number.to_string());
        git(
            &temp.0,
            &[
                "clone",
                "--no-hardlinks",
                "--",
                source,
                repo.to_str().require(Error::Io, "invalid temporary path")?,
            ],
        )?;
        for tag in crate::releases::discover(&repo, None)? {
            let id = tag.manifest.id.clone();
            if let Some(owner) = owners.insert(id.clone(), source.clone()) {
                ensure!(
                    owner == *source,
                    Catalog,
                    "multiple sources claim plugin {id}"
                );
            }
            git(&repo, &["checkout", "--detach", &tag.commit])?;
            let release = Release {
                manifest: manifest(&repo)?,
                source: source.clone(),
                commit: tag.commit,
                reference: tag.name,
                files: hash_files(&repo.join("dist"))?,
                build: None,
            };
            validate_release(&release, &id, &release.manifest.version)?;
            pending.insert((id, release.manifest.version.clone()), release);
        }
    }
    let _lock = lock(&path)?;
    let root_value = read_json(location, "catalog.json")?;
    let legacy = if let Some(value) = &root_value {
        if value.get("plugins").is_some() {
            Some(serde_json::from_value::<Catalog>(value.clone())?)
        } else {
            let directory: Directory = serde_json::from_value(value.clone())?;
            ensure!(directory.schema == 2, Catalog, "unsupported catalog schema");
            None
        }
    } else {
        None
    };
    // Check every source and immutable version before writing any release.
    for ((id, version), release) in &mut pending {
        let old = if let Some(legacy) = &legacy {
            if let Some(plugin) = legacy.plugins.get(id) {
                ensure!(
                    plugin.source == release.source,
                    Catalog,
                    "plugin {id} is registered from a different source"
                );
                plugin.versions.get(version).cloned()
            } else {
                None
            }
        } else {
            if let Some(index) = plugin_index(location, id)? {
                ensure!(
                    index.source == release.source,
                    Catalog,
                    "plugin {id} is registered from a different source"
                );
                if index.versions.contains_key(version) {
                    read_release(location, id, version, &index)?;
                }
            }
            read_json(
                location,
                &format!("{}/versions/{version}/release.json", folder(id)?),
            )?
            .map(serde_json::from_value::<Release>)
            .transpose()?
        };
        if let Some(old) = old {
            validate_release(&old, id, version)?;
            release.reference = old.reference.clone();
            release.build = old.build.clone();
            if old.files.is_empty() {
                release.files.clear();
            }
            ensure!(
                serde_json::to_value(&old)? == serde_json::to_value(&release)?,
                VersionConflict,
                "release {id}@{version} changed; refusing to replace published version"
            );
        }
    }
    prepare_root(&path)?;
    for release in pending.values() {
        write_release(path.parent().unwrap(), release)?;
        let folder = folder(&release.manifest.id)?;
        let first = folder.split('/').next().unwrap();
        link_directory(&path, first, &format!("{first}/"))?;
    }
    Ok(UpdateSourcesReport {
        releases: pending.len(),
        sources: sources.into_iter().collect(),
    })
}

/// Catalog layout version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CatalogFormat {
    /// One monolithic JSON with all plugins.
    V1,
    /// Directory tree of static JSON files.
    V2,
}
/// Plugin entry of the overview.
#[derive(Debug, Clone, Serialize)]
pub struct PluginSummary {
    pub id: String,
    pub source: String,
    /// Versions in ascending numeric order.
    pub versions: Vec<String>,
    pub latest: Option<String>,
}
/// One row of a depth-first, bounded catalog walk.
#[derive(Debug, Clone, Serialize)]
pub enum OverviewItem {
    /// A directory link `name/` at `depth`.
    Directory { depth: u32, name: String },
    /// The directory listing at `depth` has no children.
    Empty { depth: u32 },
    /// A plugin index directly below the directory at `depth` (v1: depth 1).
    Plugin { depth: u32, plugin: PluginSummary },
}
/// Bounded, read-only catalog overview.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogOverview {
    pub location: String,
    pub remote: bool,
    pub format: CatalogFormat,
    pub items: Vec<OverviewItem>,
    /// The entry limit was reached; browse a subdirectory for more.
    pub truncated: bool,
}
pub const OVERVIEW_LIMIT: usize = 40;

fn summary(
    id: &str,
    source: &str,
    versions: impl Iterator<Item = String>,
) -> Result<PluginSummary> {
    valid_id(id)?;
    let mut versions = versions
        .map(|v| Ok((version_key(&v)?, v)))
        .collect::<Result<Vec<_>>>()?;
    versions.sort();
    let versions: Vec<String> = versions.into_iter().map(|(_, v)| v).collect();
    Ok(PluginSummary {
        id: id.into(),
        source: source.into(),
        latest: versions.last().cloned(),
        versions,
    })
}

/// Bounded, read-only overview. Follows only validated relative directory links.
pub fn browse(location: &str) -> Result<CatalogOverview> {
    fn walk(
        location: &str,
        prefix: &str,
        value: serde_json::Value,
        depth: u32,
        remaining: &mut usize,
        items: &mut Vec<OverviewItem>,
    ) -> Result<()> {
        ensure!(depth <= 8, Catalog, "catalog nesting is too deep");
        let directory: Directory =
            serde_json::from_value(value).context(Error::Catalog, "invalid directory catalog")?;
        ensure!(directory.schema == 2, Catalog, "unsupported catalog schema");
        if directory.directories.is_empty() {
            items.push(OverviewItem::Empty { depth });
        }
        for (name, link) in directory.directories {
            if *remaining == 0 {
                break;
            }
            identity::valid_part(&name)?;
            ensure!(
                link == format!("{name}/"),
                Catalog,
                "invalid directory link: {link}"
            );
            *remaining -= 1;
            let child = format!("{prefix}{link}");
            items.push(OverviewItem::Directory {
                depth,
                name: name.clone(),
            });
            if let Some(value) = read_json(location, &format!("{child}plugin.json"))? {
                let plugin: PluginIndex = serde_json::from_value(value)?;
                ensure!(plugin.schema == 2, Catalog, "unsupported plugin schema");
                for (version, link) in &plugin.versions {
                    version_key(version)?;
                    ensure!(
                        *link == format!("versions/{version}/release.json"),
                        Catalog,
                        "invalid release link"
                    );
                }
                items.push(OverviewItem::Plugin {
                    depth,
                    plugin: summary(&plugin.id, &plugin.source, plugin.versions.into_keys())?,
                });
            } else {
                let value =
                    read_json(location, &format!("{child}catalog.json"))?.ok_or_else(|| {
                        Error::Catalog(format!("missing catalog/plugin index in {child}"))
                    })?;
                walk(location, &child, value, depth + 1, remaining, items)?;
            }
        }
        Ok(())
    }
    let remote = local_file(location)?.is_none();
    let value = list(location)?;
    let mut remaining = OVERVIEW_LIMIT;
    let mut items = Vec::new();
    let format = if value.get("plugins").is_some() {
        let catalog: Catalog = serde_json::from_value(value)?;
        for (id, plugin) in catalog.plugins.iter().take(OVERVIEW_LIMIT) {
            items.push(OverviewItem::Plugin {
                depth: 1,
                plugin: summary(id, &plugin.source, plugin.versions.keys().cloned())?,
            });
        }
        if catalog.plugins.is_empty() {
            items.push(OverviewItem::Empty { depth: 1 });
        }
        if catalog.plugins.len() > OVERVIEW_LIMIT {
            remaining = 0;
        }
        CatalogFormat::V1
    } else {
        walk(location, "", value, 1, &mut remaining, &mut items)?;
        CatalogFormat::V2
    };
    Ok(CatalogOverview {
        location: location.into(),
        remote,
        format,
        items,
        truncated: remaining == 0,
    })
}
