//! Catalog client and author operations. The catalog stores metadata, never code.
use crate::error::{Context, Error, Require, Result, ensure, fail};
use crate::{
    Manifest, identity, model::package_manifest, model::resolve_ref, packages, transport, valid_id,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
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
    /// Package folder in a multi-package repository (`gitpm.json`); absent: the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub source: String,
    pub versions: BTreeMap<String, Release>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub packages: BTreeMap<String, Package>,
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
    /// Only in the top-level `catalog.json`: set by the catalog's owner by hand. Writers
    /// keep it; a verified catalog takes versions only through [`register`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    verified: bool,
    directories: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageIndex {
    schema: u32,
    id: String,
    source: String,
    versions: BTreeMap<String, String>,
    // Display-only data stays untyped here so a malformed summary never prevents
    // installs or hides other packages in an overview.
    #[serde(default)]
    latest: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexSummary {
    version: String,
    title: Option<String>,
    description: Option<String>,
    provides: Vec<String>,
    tags: Vec<String>,
}
impl IndexSummary {
    fn from_release(release: &Release) -> Self {
        let metadata = release.metadata();
        Self {
            version: release.manifest.version.clone(),
            title: metadata.title,
            description: metadata.description,
            provides: metadata.provides,
            tags: metadata.tags,
        }
    }
}
fn folder(id: &str) -> Result<String> {
    valid_id(id)?;
    Ok(format!("namespaces/{id}"))
}
/// The root `catalog.json` at `location` as a directory file; anything else (a v1
/// monolithic catalog included) is refused with `catalog`.
fn root_directory(location: &str) -> Result<Directory> {
    directory_of(list(location)?)
}
/// A directory file `{"schema": 1, "directories": {…}}`.
fn directory_of(value: serde_json::Value) -> Result<Directory> {
    ensure!(
        value.get("plugins").is_none(),
        Catalog,
        "unsupported catalog schema: v1 catalogs are no longer read; regenerate the catalog from the release tags"
    );
    let directory: Directory =
        serde_json::from_value(value).context(Error::Catalog, "invalid directory catalog")?;
    ensure!(directory.schema == 1, Catalog, "unsupported catalog schema");
    Ok(directory)
}
/// Lowercase hex of a digest (sha2 0.11 output no longer implements `LowerHex`).
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
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
                    sha256: hex(&hasher.finalize()),
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

pub(crate) struct HashedDist {
    pub files: BTreeMap<String, FileDigest>,
    pub warnings: Vec<String>,
}

/// Hash the same committed export that a workspace stages at `dist/<id>`.
pub(crate) fn hash_exported_dist(
    repo: &Path,
    commit: &str,
    package: Option<&str>,
    staged: &Path,
    custom_ssh: bool,
) -> Result<HashedDist> {
    if staged.exists() {
        fs::remove_dir_all(staged)?;
    }
    let warnings = crate::workspace::export_dist(repo, commit, package, staged, custom_ssh)
        .map_err(|error| match error {
            Error::Unsupported(_) => Error::Unsupported("build contains a symlink".into()),
            other => other,
        })?;
    Ok(HashedDist {
        files: hash_files(staged)?,
        warnings,
    })
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn validate_release(release: &Release, id: &str, version: &str) -> Result<()> {
    valid_id(id)?;
    version_key(version)?;
    ensure!(
        release.manifest.id == id && release.manifest.version == version,
        Catalog,
        "catalog identity mismatch"
    );
    validate_release_payload(release, id)
}

fn validate_release_payload(release: &Release, id: &str) -> Result<()> {
    ensure!(
        matches!(release.commit.len(), 40 | 64)
            && release.commit.bytes().all(|c| c.is_ascii_hexdigit()),
        Catalog,
        "invalid catalog commit"
    );
    // Every release carries file hashes, or dist could not be verified on install.
    ensure!(
        !release.files.is_empty(),
        Catalog,
        "catalog release of {id} has no file hashes"
    );
    if let Some(path) = &release.path {
        crate::packages::validate_path(path)
            .map_err(|_| Error::Catalog(format!("invalid package path {path:?}")))?;
    }
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
            (build.url.starts_with("https://")
                || (build.url.starts_with("http://") && ensure_secure_url(&build.url).is_ok()))
                && build.url.len() > 8
                && !build.url.chars().any(char::is_whitespace),
            Catalog,
            "build URL must use HTTPS"
        );
        ensure!(valid_hash(&build.sha256), Catalog, "invalid build SHA-256");
    }
    Ok(())
}

/// Parse a package version: SemVer `MAJOR.MINOR.PATCH` with an optional pre-release
/// `-PRE` (dot-separated identifiers of ASCII letters, digits and hyphens; numbers
/// without leading zeros), never build metadata. The result orders by SemVer
/// precedence (`1.0.0-alpha < 1.0.0-beta < 1.0.0`).
pub fn version_key(version: &str) -> Result<semver::Version> {
    let invalid = || {
        Error::InvalidVersion(
            "version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-PRE without leading zeros or build metadata"
                .into(),
        )
    };
    let number = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|c| c.is_ascii_digit())
            && (part.len() == 1 || !part.starts_with('0'))
    };
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let parts: Vec<_> = core.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| number(p)) {
        return Err(invalid());
    }
    let pre = match pre {
        None => semver::Prerelease::EMPTY,
        Some(pre) => {
            let valid = pre.split('.').all(|identifier| {
                !identifier.is_empty()
                    && identifier
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    && (!identifier.bytes().all(|c| c.is_ascii_digit()) || number(identifier))
            });
            if !valid {
                return Err(invalid());
            }
            semver::Prerelease::new(pre).map_err(|_| invalid())?
        }
    };
    let parse = |part: &str| part.parse::<u64>().map_err(|_| invalid());
    Ok(semver::Version {
        major: parse(parts[0])?,
        minor: parse(parts[1])?,
        patch: parse(parts[2])?,
        pre,
        build: semver::BuildMetadata::EMPTY,
    })
}

/// The version `latest` stands for among `versions` (unparsable ones ignored): the
/// newest release that is not a pre-release, or, when `installed` is a pre-release,
/// also a newer pre-release of the same `MAJOR.MINOR.PATCH` (see
/// [`crate::deps::follows`]).
pub fn newest_eligible<'a>(
    versions: impl IntoIterator<Item = &'a str>,
    installed: Option<&str>,
) -> Option<&'a str> {
    let installed = installed.and_then(|v| version_key(v).ok());
    versions
        .into_iter()
        .filter_map(|v| Some((version_key(v).ok()?, v)))
        .filter(|(key, _)| crate::deps::follows(installed.as_ref(), key))
        .max()
        .map(|(_, v)| v)
}

/// The newest pre-release among `versions` that is newer than `than` and `installed`
/// and not eligible for `installed` (see [`newest_eligible`]): shown, never offered.
pub fn newest_prerelease<'a>(
    versions: impl IntoIterator<Item = &'a str>,
    installed: Option<&str>,
    than: &str,
) -> Option<&'a str> {
    let installed = installed.and_then(|v| version_key(v).ok());
    let than = version_key(than).ok()?;
    versions
        .into_iter()
        .filter_map(|v| Some((version_key(v).ok()?, v)))
        .filter(|(key, _)| {
            !key.pre.is_empty()
                && *key > than
                && installed.as_ref().is_none_or(|installed| key > installed)
                && !crate::deps::follows(installed.as_ref(), key)
        })
        .max()
        .map(|(_, v)| v)
}

/// Refusal of a catalog write to an HTTP(S) location (catalogs are written only locally).
const WRITE_NEEDS_LOCAL: &str =
    "writing a catalog requires a local folder; HTTP(S) catalogs are read-only";
/// Check a catalog location before it is used for reading: an HTTP(S) URL must be
/// secure (see the transport rules), anything else is a local folder or JSON file that
/// must exist ([`Error::InvalidArgument`] "catalog location PATH does not exist"). A
/// missing local catalog never counts as "no catalog"; only the catalog writers create it.
pub fn validate_location(location: &str) -> Result<()> {
    if local_file(location)?.is_some() {
        ensure!(
            fs::symlink_metadata(location).is_ok(),
            InvalidArgument,
            "catalog location {location} does not exist"
        );
    }
    Ok(())
}
/// True if `location` is a local folder or JSON file (not HTTP).
pub fn is_local(location: &str) -> Result<bool> {
    Ok(local_file(location)?.is_some())
}
/// Plain HTTP lets anyone on the path swap commits and sources, so remote catalogs
/// must use HTTPS. `http://` is accepted only for loopback test servers.
fn ensure_secure_url(url: &str) -> Result<()> {
    if let Some(rest) = url.strip_prefix("http://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host = match authority.rsplit_once('@') {
            Some(_) => "",
            None if authority.starts_with('[') => {
                authority.split_once(']').map_or("", |(h, _)| &h[1..])
            }
            None => authority.split(':').next().unwrap_or_default(),
        };
        ensure!(
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()),
            InvalidArgument,
            "catalog URL must use HTTPS (http:// only for localhost)"
        );
    }
    Ok(())
}
fn local_file(location: &str) -> Result<Option<PathBuf>> {
    if location.starts_with("http://") || location.starts_with("https://") {
        ensure_secure_url(location)?;
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
        match transport::http_get(&url)? {
            Some(bytes) => bytes,
            None => return Ok(None),
        }
    };
    Ok(Some(
        serde_json::from_slice(&bytes).context(Error::Catalog, "invalid catalog JSON")?,
    ))
}
/// The directory links at one catalog level: its root `catalog.json` (schema 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogLinks {
    pub schema: u32,
    /// Directory name → relative link of its own `catalog.json` level.
    pub directories: BTreeMap<String, String>,
}
/// [`CatalogLinks`] of the catalog level at `location` (refused with `catalog` when its
/// `catalog.json` is missing or not a directory file).
pub fn links(location: &str) -> Result<CatalogLinks> {
    let directory = root_directory(location)?;
    Ok(CatalogLinks {
        schema: directory.schema,
        directories: directory.directories,
    })
}
/// Whether the catalog at `location` is verified: its top-level `catalog.json` has
/// `"verified": true` (set by its owner by hand). A verified catalog protects the
/// packages installed from it; any other catalog serves discovery only.
pub fn is_verified(location: &str) -> Result<bool> {
    Ok(root_directory(location)?.verified)
}
/// Refusal of bulk generation into a verified catalog.
const VERIFIED_TAKES_REGISTER_ONLY: &str =
    "a verified catalog takes versions only one by one through register";
/// Raw root `catalog.json` at this catalog level.
pub fn list(location: &str) -> Result<serde_json::Value> {
    validate_location(location)?;
    read_json(location, "catalog.json")?.require(Error::Catalog, "catalog not found")
}
fn package_index(location: &str, id: &str) -> Result<Option<PackageIndex>> {
    let Some(value) = read_json(location, &format!("{}/plugin.json", folder(id)?))? else {
        return Ok(None);
    };
    let index: PackageIndex = serde_json::from_value(value)?;
    ensure!(
        index.schema == 1 && index.id == id,
        Catalog,
        "invalid package index"
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
/// Catalog entries relevant to the installed packages: latest and currently installed versions.
pub fn selected(location: &str, state: &crate::State) -> Result<Catalog> {
    selected_impl(location, state, false)
}

/// Read-only diagnostics retain identity mismatches for per-package status reporting.
/// Transport, syntax and unsafe payload errors still fail the operation.
pub(crate) fn selected_for_check(location: &str, state: &crate::State) -> Result<Catalog> {
    selected_impl(location, state, true)
}

fn selected_impl(location: &str, state: &crate::State, diagnostic: bool) -> Result<Catalog> {
    validate_location(location)?;
    let mut result = Catalog::default();
    // Installed packages with their current and latest release; originals of forks
    // with their latest release only.
    let mut wanted: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    for installed in state.packages.values() {
        if let Some(base) = &installed.fork_of {
            wanted.entry(base.id.as_str()).or_insert(None);
        }
    }
    for (id, installed) in &state.packages {
        wanted.insert(id, Some(installed.current.version.as_str()));
    }
    for (id, current) in wanted {
        if let Some(index) = package_index(location, id)? {
            let mut versions = BTreeMap::new();
            // `check` compares with the newest version offered to the installed one and
            // notes the newest pre-release.
            let all = || index.versions.keys().map(String::as_str);
            let latest = newest_eligible(all(), current);
            let prerelease =
                newest_prerelease(all(), current, latest.or(current).unwrap_or("0.0.0"));
            let latest = latest.and_then(|v| index.versions.get_key_value(v).map(|(v, _)| v));
            let prerelease =
                prerelease.and_then(|v| index.versions.get_key_value(v).map(|(v, _)| v));
            for version in latest
                .into_iter()
                .chain(prerelease)
                .chain(current.and_then(|c| index.versions.get_key_value(c).map(|(v, _)| v)))
            {
                if !versions.contains_key(version) {
                    versions.insert(
                        version.clone(),
                        read_release_impl(location, id, version, &index, diagnostic)?,
                    );
                }
            }
            result.packages.insert(
                id.to_owned(),
                Package {
                    source: index.source,
                    versions,
                },
            );
        } else {
            root_directory(location)?;
        }
    }
    Ok(result)
}
fn read_release(location: &str, id: &str, version: &str, index: &PackageIndex) -> Result<Release> {
    read_release_impl(location, id, version, index, false)
}

fn read_release_impl(
    location: &str,
    id: &str,
    version: &str,
    index: &PackageIndex,
    diagnostic: bool,
) -> Result<Release> {
    let link = index.versions.get(version).require(
        Error::Catalog,
        format!("version not found in catalog: {id}@{version}"),
    )?;
    let release: Release = serde_json::from_value(
        read_json(location, &format!("{}/{link}", folder(id)?))?
            .require(Error::Catalog, "release not found")?,
    )?;
    if diagnostic {
        validate_release_payload(&release, id)?;
    } else {
        validate_release(&release, id, version)?;
        ensure!(
            release.source == index.source,
            Catalog,
            "release source mismatch"
        );
    }
    Ok(release)
}

/// A catalog directory an external uploader may publish; returns its canonical path.
pub fn ensure_publishable(location: &str) -> Result<PathBuf> {
    ensure!(
        local_file(location)?.is_some() && Path::new(location).is_dir(),
        InvalidArgument,
        "a publishable catalog is a local folder containing catalog.json"
    );
    Ok(fs::canonicalize(location)?)
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    crate::atomic_write::write_atomically(
        path,
        path,
        format!("{}\n", serde_json::to_string_pretty(value)?).as_bytes(),
    )
}
/// Writer lock of a local catalog. Released with the process, so a killed writer
/// never blocks the catalog; its leftover temporary files are removed here.
fn lock(path: &Path) -> Result<crate::lock::FileLock> {
    let root = path.parent().unwrap();
    fs::create_dir_all(root)?;
    // Beside the catalog folder, so the published tree stays clean.
    let root = fs::canonicalize(root)?;
    let name = root
        .file_name()
        .require(Error::InvalidArgument, "catalog needs a folder name")?
        .to_string_lossy()
        .into_owned();
    let lock = crate::lock::try_lock(
        &root
            .parent()
            .require(Error::InvalidArgument, "catalog needs a parent folder")?
            .join(format!(".{name}.catalog.lock")),
        "catalog locked",
    )?;
    remove_stale_temporaries(&root)?;
    Ok(lock)
}
/// Only called under the catalog lock: every `*.tmp` is then from a dead writer.
fn remove_stale_temporaries(dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if kind.is_dir() {
            remove_stale_temporaries(&entry.path())?;
        } else if kind.is_file() && name.to_string_lossy().ends_with(".tmp") {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
fn link_directory(path: &Path, name: &str, link: &str) -> Result<()> {
    let mut directory: Directory = if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        Directory {
            schema: 1,
            ..Default::default()
        }
    };
    ensure!(directory.schema == 1, Catalog, "unsupported catalog schema");
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
    let mut index: PackageIndex = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        PackageIndex {
            schema: 1,
            id: id.clone(),
            source: release.source.clone(),
            versions: BTreeMap::new(),
            latest: serde_json::Value::Null,
        }
    };
    ensure!(
        index.schema == 1 && index.id == *id && index.source == release.source,
        Catalog,
        "package {id} is registered from a different source or identity"
    );
    let link = format!("versions/{version}/release.json");
    let release_path = root.join(&folder).join(&link);
    if release_path.exists() {
        let old: Release = serde_json::from_slice(&fs::read(&release_path)?)?;
        let mut candidate = release.clone();
        // Alternate Git refs can resolve to the same immutable commit.
        candidate.reference = old.reference.clone();
        ensure!(
            serde_json::to_value(&old)? == serde_json::to_value(&candidate)?,
            VersionConflict,
            "version {id}@{version} already exists and is immutable"
        );
    } else {
        atomic_json(&release_path, release)?;
    }
    if !index.versions.contains_key(version) {
        index.versions.insert(version.clone(), link);
    }
    let newest = summary_version(index.versions.keys().map(String::as_str))
        .require(Error::Catalog, "package has no versions")?;
    let newest_release: Release =
        serde_json::from_slice(&fs::read(root.join(&folder).join(&index.versions[newest]))?)?;
    validate_release(&newest_release, id, newest)?;
    index.latest = serde_json::to_value(IndexSummary::from_release(&newest_release))?;
    atomic_json(&path, &index)?;
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
                schema: 1,
                ..Default::default()
            },
        );
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?).context(
        Error::Catalog,
        "invalid existing catalog; refusing overwrite",
    )?;
    directory_of(value)?;
    Ok(())
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
/// Published versions of `id` (unordered); empty if the catalog does not know the package.
pub fn versions(location: &str, id: &str) -> Result<Vec<String>> {
    valid_id(id)?;
    validate_location(location)?;
    if let Some(index) = package_index(location, id)? {
        return Ok(index.versions.into_keys().collect());
    }
    root_directory(location)?;
    Ok(Vec::new())
}

/// One spelling per catalog, for remembering where a package came from: the real path
/// of a local `catalog.json`, or an HTTP(S) base URL without `/catalog.json`.
pub fn canonical_location(location: &str) -> Result<String> {
    Ok(match local_file(location)? {
        Some(path) => fs::canonicalize(&path)
            .context(
                Error::Catalog,
                format!("catalog not found: {}", path.display()),
            )?
            .to_string_lossy()
            .into_owned(),
        None => location
            .trim_end_matches('/')
            .trim_end_matches("/catalog.json")
            .trim_end_matches('/')
            .to_owned(),
    })
}

/// Exact release of `id` (the latest one if `version` is `None`).
pub fn lookup(location: &str, id: &str, version: Option<&str>) -> Result<Release> {
    valid_id(id)?;
    if let Some(version) = version {
        version_key(version)?;
    }
    validate_location(location)?;
    if let Some(index) = package_index(location, id)? {
        let version = match version {
            Some(v) => v,
            // The latest is the newest release; a pre-release is taken only by name.
            None => match newest_eligible(index.versions.keys().map(String::as_str), None) {
                Some(v) => v,
                None if index.versions.is_empty() => fail!(Catalog, "package has no versions"),
                None => fail!(
                    Catalog,
                    "{id} has only pre-release versions in the catalog; name one with a version"
                ),
            },
        };
        return read_release(location, id, version, &index);
    }
    root_directory(location)?;
    fail!(Catalog, "package not found in catalog: {id}")
}
struct Scratch(PathBuf);
impl Scratch {
    /// Unique directory under the OS temporary directory (also across threads).
    fn new(prefix: &str) -> Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    register_with_ssh(url, source, reference, false)
}

/// Run with the caller's SSH policy.
pub fn register_with_options(
    url: &str,
    source: &str,
    reference: &str,
    ssh_options: &crate::Options,
) -> Result<Release> {
    register_with_ssh(url, source, reference, ssh_options.custom_ssh)
}

pub(crate) fn register_with_ssh(
    url: &str,
    source: &str,
    reference: &str,
    custom_ssh: bool,
) -> Result<Release> {
    register_build_with_ssh(url, source, reference, None, custom_ssh)
}
pub fn register_build(
    url: &str,
    source: &str,
    reference: &str,
    build: Option<Build>,
) -> Result<Release> {
    register_build_with_ssh(url, source, reference, build, false)
}

/// Run with the caller's SSH policy.
pub fn register_build_with_options(
    url: &str,
    source: &str,
    reference: &str,
    build: Option<Build>,
    ssh_options: &crate::Options,
) -> Result<Release> {
    register_build_with_ssh(url, source, reference, build, ssh_options.custom_ssh)
}

pub(crate) fn register_build_with_ssh(
    url: &str,
    source: &str,
    reference: &str,
    build: Option<Build>,
    custom_ssh: bool,
) -> Result<Release> {
    register_package_with_ssh(url, source, reference, None, build, custom_ssh)
}
/// [`register_build`] of one package of a multi-package repository: `package` picks it
/// (an `ID@VERSION` reference names it too); for a single-package repository a given
/// `package` must match the root manifest.
pub fn register_package(
    url: &str,
    source: &str,
    reference: &str,
    package: Option<&str>,
    build: Option<Build>,
) -> Result<Release> {
    register_package_with_ssh(url, source, reference, package, build, false)
}

/// Run with the caller's SSH policy.
pub fn register_package_with_options(
    url: &str,
    source: &str,
    reference: &str,
    package: Option<&str>,
    build: Option<Build>,
    ssh_options: &crate::Options,
) -> Result<Release> {
    register_package_with_ssh(
        url,
        source,
        reference,
        package,
        build,
        ssh_options.custom_ssh,
    )
}

pub(crate) fn register_package_with_ssh(
    url: &str,
    source: &str,
    reference: &str,
    package: Option<&str>,
    build: Option<Build>,
    custom_ssh: bool,
) -> Result<Release> {
    Ok(register_package_report_with_options(
        url,
        source,
        reference,
        package,
        build,
        &crate::Options {
            custom_ssh,
            ..Default::default()
        },
    )?
    .release)
}

/// Publication result; warnings are not stored in the catalog release.
#[derive(Debug, Clone, Serialize)]
pub struct RegisterReport {
    pub release: Release,
    pub warnings: Vec<String>,
}

/// Register a committed package and report warnings about omitted submodule contents.
pub fn register_package_report_with_options(
    url: &str,
    source: &str,
    reference: &str,
    package: Option<&str>,
    build: Option<Build>,
    ssh_options: &crate::Options,
) -> Result<RegisterReport> {
    if let Some(build) = &build {
        ensure!(
            build.url.starts_with("https://"),
            Catalog,
            "build URL must use HTTPS"
        );
    }
    let custom_ssh = ssh_options.custom_ssh;
    let catalog_path = local_file(url)?.require(Error::InvalidArgument, WRITE_NEEDS_LOCAL)?;
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
    transport::clone_repo(&temp.0, &source, "repo", custom_ssh)?;
    let repo = temp.0.join("repo");
    let commit = resolve_ref(&repo, reference, custom_ssh)?;
    let found = packages::find_at_with_ssh(
        &repo,
        &commit,
        package.or_else(|| packages::tag_id(reference)),
        custom_ssh,
    )?;
    let path = found.subdir().map(str::to_owned);
    transport::checkout_detached(&repo, &commit, custom_ssh)?;
    let manifest = package_manifest(&repo, path.as_deref())?;
    version_key(&manifest.version)?;
    let exported = hash_exported_dist(
        &repo,
        &commit,
        path.as_deref(),
        &temp.0.join("staged-dist"),
        custom_ssh,
    )?;
    let release = Release {
        manifest,
        source,
        commit,
        reference: reference.to_owned(),
        files: exported.files,
        build,
        path,
    };
    Ok(RegisterReport {
        release: save_release(&catalog_path, &release)?,
        warnings: exported.warnings,
    })
}

/// Packages whose releases a source publishes: `[None]` (the root package) for a
/// single-package repository, every package listed at HEAD for a `gitpm.json` one. As
/// for a root package, HEAD decides the identity: tags a fork keeps from its upstream
/// are not releases of the fork. Folders at HEAD that are not valid packages are
/// returned as warnings (the other packages are still published).
fn source_packages(
    repo: &Path,
    custom_ssh: bool,
) -> Result<(Vec<Option<String>>, Vec<crate::releases::SkippedTag>)> {
    if packages::config_at_with_ssh(repo, "HEAD", custom_ssh)?.is_none() {
        return Ok((vec![None], Vec::new()));
    }
    let head = transport::object_id(repo, "HEAD", custom_ssh)?;
    let layout = packages::layout_at_with_ssh(repo, &head, custom_ssh)?.unwrap_or_default();
    let warnings = layout
        .problems
        .iter()
        .map(|problem| crate::releases::SkippedTag {
            name: format!("HEAD:{}", problem.path),
            version: String::new(),
            reason: format!("{} is not a package: {}", problem.path, problem.error),
        })
        .collect();
    Ok((
        layout
            .packages
            .into_iter()
            .map(|p| Some(p.manifest.id))
            .collect(),
        warnings,
    ))
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
    /// Release-looking tags that were skipped (broken manifest, version mismatch).
    pub skipped: Vec<SkippedSourceTag>,
    /// Published versions whose regenerated release differs from the record (a moved
    /// tag, or a manifest or file hashes that changed): the published record is kept.
    /// Also includes warnings about submodules whose contents are not installed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// A skipped tag of one source in [`UpdateSourcesReport`].
#[derive(Debug, Clone, Serialize)]
pub struct SkippedSourceTag {
    pub source: String,
    #[serde(flatten)]
    pub tag: crate::releases::SkippedTag,
}

/// `catalog-update` never writes into a verified catalog (an existing top-level
/// `catalog.json` must also be a valid directory file).
fn refuse_verified(location: &str) -> Result<()> {
    if let Some(value) = read_json(location, "catalog.json")? {
        ensure!(
            !directory_of(value)?.verified,
            InvalidArgument,
            "{VERIFIED_TAKES_REGISTER_ONLY}"
        );
    }
    Ok(())
}
/// Read-only source discovery and hashing precede any catalog changes. A verified
/// catalog is refused with `invalid_argument` before anything is read or written.
/// Removal from the source list never deletes already published releases.
pub fn update_sources(
    location: &str,
    sources_file: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<UpdateSourcesReport> {
    update_sources_with_ssh(location, sources_file, progress, false)
}

/// Run with the caller's SSH policy.
pub fn update_sources_with_options(
    location: &str,
    sources_file: &Path,
    progress: &mut dyn FnMut(Progress),
    ssh_options: &crate::Options,
) -> Result<UpdateSourcesReport> {
    update_sources_with_ssh(location, sources_file, progress, ssh_options.custom_ssh)
}

pub(crate) fn update_sources_with_ssh(
    location: &str,
    sources_file: &Path,
    progress: &mut dyn FnMut(Progress),
    custom_ssh: bool,
) -> Result<UpdateSourcesReport> {
    let path = local_file(location)?.require(Error::InvalidArgument, WRITE_NEEDS_LOCAL)?;
    refuse_verified(location)?;
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
    let mut warnings = Vec::new();
    let mut pending = BTreeMap::new();
    let mut owners = BTreeMap::new();
    let mut skipped = Vec::new();
    for (number, source) in sources.iter().enumerate() {
        progress(Progress::ReadingReleases {
            source: source.clone(),
        });
        let repo = temp.0.join(number.to_string());
        transport::clone_repo(
            &temp.0,
            source,
            repo.to_str().require(Error::Io, "invalid temporary path")?,
            custom_ssh,
        )?;
        let mut tags = Vec::new();
        let (ids, source_warnings) = source_packages(&repo, custom_ssh)?;
        skipped.extend(source_warnings.into_iter().map(|tag| SkippedSourceTag {
            source: source.clone(),
            tag,
        }));
        for id in ids {
            let found =
                match crate::releases::discover_all_with_ssh(&repo, id.as_deref(), custom_ssh) {
                    // A package listed at HEAD without a release yet has nothing to publish.
                    Err(Error::NoReleases(_)) if id.is_some() => continue,
                    other => other?,
                };
            found.ensure_latest_is_valid()?;
            skipped.extend(found.skipped.into_iter().map(|tag| SkippedSourceTag {
                source: source.clone(),
                tag,
            }));
            tags.extend(found.tags);
        }
        for tag in tags {
            let id = tag.manifest.id.clone();
            if let Some(owner) = owners.insert(id.clone(), source.clone()) {
                ensure!(
                    owner == *source,
                    Catalog,
                    "multiple sources claim package {id}"
                );
            }
            transport::checkout_detached(&repo, &tag.commit, custom_ssh)?;
            let path = (!tag.path.is_empty()).then_some(tag.path);
            let exported = hash_exported_dist(
                &repo,
                &tag.commit,
                path.as_deref(),
                &temp.0.join("staged-dist"),
                custom_ssh,
            )?;
            warnings.extend(exported.warnings);
            let release = Release {
                manifest: package_manifest(&repo, path.as_deref())?,
                source: source.clone(),
                commit: tag.commit,
                reference: tag.name,
                files: exported.files,
                build: None,
                path,
            };
            validate_release(&release, &id, &release.manifest.version)?;
            pending.insert((id, release.manifest.version.clone()), release);
        }
    }
    let _lock = lock(&path)?;
    refuse_verified(location)?;
    // Check every source and immutable version before writing any release.
    for ((id, version), release) in &mut pending {
        if let Some(index) = package_index(location, id)? {
            ensure!(
                index.source == release.source,
                Catalog,
                "package {id} is registered from a different source"
            );
            if index.versions.contains_key(version) {
                read_release(location, id, version, &index)?;
            }
            // Summary regeneration can also read a published version whose tag
            // disappeared upstream. Validate it before publishing any package.
            if let Some(latest) = summary_version(index.versions.keys().map(String::as_str)) {
                read_release(location, id, latest, &index)?;
            }
        }
        let old = read_json(
            location,
            &format!("{}/versions/{version}/release.json", folder(id)?),
        )?
        .map(serde_json::from_value::<Release>)
        .transpose()?;
        if let Some(old) = old {
            validate_release(&old, id, version)?;
            release.reference = old.reference.clone();
            release.build = old.build.clone();
            // A published record is never replaced: a moved tag or a regenerated
            // release that differs keeps the record, with a warning.
            if serde_json::to_value(&old)? != serde_json::to_value(&*release)? {
                warnings.push(if old.commit != release.commit {
                    format!(
                        "release {id}@{version}: tag now points to {}; the published record is kept",
                        &release.commit[..release.commit.len().min(12)]
                    )
                } else {
                    format!(
                        "release {id}@{version} differs from the published record; the published record is kept"
                    )
                });
                // Re-publish the index from the immutable record as well: an old
                // catalog may still need its display summary regenerated.
                *release = old;
            }
        }
    }
    let releases = pending.len();
    prepare_root(&path)?;
    for release in pending.values() {
        write_release(path.parent().unwrap(), release)?;
        let folder = folder(&release.manifest.id)?;
        let first = folder.split('/').next().unwrap();
        link_directory(&path, first, &format!("{first}/"))?;
    }
    Ok(UpdateSourcesReport {
        releases,
        sources: sources.into_iter().collect(),
        skipped,
        warnings,
    })
}

/// Package entry of the overview.
#[derive(Debug, Clone, Serialize)]
pub struct PackageSummary {
    pub id: String,
    pub source: String,
    /// Versions in ascending SemVer order, pre-releases included.
    pub versions: Vec<String>,
    /// The newest version that is not a pre-release (what `install-catalog` without a
    /// version installs); `None` when there is none.
    pub latest: Option<String>,
    /// Descriptive metadata of the latest release's manifest (`plugin.json`).
    pub title: Option<String>,
    pub description: Option<String>,
    pub provides: Vec<String>,
    pub tags: Vec<String>,
}
impl Release {
    /// Descriptive metadata of the released manifest (lenient, see
    /// [`Manifest::metadata`]).
    pub fn metadata(&self) -> crate::Metadata {
        self.manifest.metadata()
    }
}
/// One row of a depth-first, bounded catalog walk.
#[derive(Debug, Clone, Serialize)]
pub enum OverviewItem {
    /// A directory link `name/` at `depth`.
    Directory { depth: u32, name: String },
    /// The directory listing at `depth` has no children.
    Empty { depth: u32 },
    /// A package index directly below the directory at `depth`.
    Package { depth: u32, package: PackageSummary },
}
/// Bounded, read-only catalog overview.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogOverview {
    pub location: String,
    pub remote: bool,
    /// The top-level `catalog.json` has `"verified": true` (see [`is_verified`]).
    pub verified: bool,
    pub items: Vec<OverviewItem>,
    /// The entry limit was reached; browse a subdirectory for more.
    pub truncated: bool,
}
pub const OVERVIEW_LIMIT: usize = 40;

// Metadata follows the newest stable version, falling back to the newest
// prerelease only for a package with no stable versions.
fn summary_version<'a>(versions: impl Iterator<Item = &'a str> + Clone) -> Option<&'a str> {
    newest_eligible(versions.clone(), None).or_else(|| {
        versions
            .filter_map(|v| version_key(v).ok().map(|key| (key, v)))
            .max()
            .map(|(_, v)| v)
    })
}

fn summary(
    id: &str,
    source: &str,
    versions: impl Iterator<Item = String>,
    latest: serde_json::Value,
) -> Result<PackageSummary> {
    valid_id(id)?;
    let mut versions = versions
        .map(|v| Ok((version_key(&v)?, v)))
        .collect::<Result<Vec<_>>>()?;
    versions.sort();
    let versions: Vec<String> = versions.into_iter().map(|(_, v)| v).collect();
    let newest = newest_eligible(versions.iter().map(String::as_str), None).map(str::to_owned);
    // A summary must describe the selected version; invalid display data only
    // removes this package's metadata and never triggers a release request.
    let metadata = serde_json::from_value::<IndexSummary>(latest)
        .ok()
        .filter(|latest| {
            Some(latest.version.as_str()) == summary_version(versions.iter().map(String::as_str))
        });
    let metadata = metadata
        .map(|latest| crate::Metadata {
            title: latest.title,
            description: latest.description,
            provides: latest.provides,
            tags: latest.tags,
        })
        .unwrap_or_default();
    Ok(PackageSummary {
        id: id.into(),
        source: source.into(),
        latest: newest,
        versions,
        title: metadata.title,
        description: metadata.description,
        provides: metadata.provides,
        tags: metadata.tags,
    })
}

/// Bounded, read-only overview. Follows only validated relative directory links.
pub fn browse(location: &str) -> Result<CatalogOverview> {
    validate_location(location)?;
    fn walk(
        location: &str,
        prefix: &str,
        value: serde_json::Value,
        depth: u32,
        remaining: &mut usize,
        items: &mut Vec<OverviewItem>,
    ) -> Result<()> {
        ensure!(depth <= 8, Catalog, "catalog nesting is too deep");
        let directory = directory_of(value)?;
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
                let plugin: PackageIndex = serde_json::from_value(value)?;
                ensure!(plugin.schema == 1, Catalog, "unsupported package schema");
                for (version, link) in &plugin.versions {
                    version_key(version)?;
                    ensure!(
                        *link == format!("versions/{version}/release.json"),
                        Catalog,
                        "invalid release link"
                    );
                }
                let entry = summary(
                    &plugin.id,
                    &plugin.source,
                    plugin.versions.keys().cloned(),
                    plugin.latest,
                )?;
                items.push(OverviewItem::Package {
                    depth,
                    package: entry,
                });
            } else {
                let value =
                    read_json(location, &format!("{child}catalog.json"))?.ok_or_else(|| {
                        Error::Catalog(format!("missing catalog/package index in {child}"))
                    })?;
                walk(location, &child, value, depth + 1, remaining, items)?;
            }
        }
        Ok(())
    }
    let remote = local_file(location)?.is_none();
    let value = list(location)?;
    let verified = directory_of(value.clone())?.verified;
    let mut remaining = OVERVIEW_LIMIT;
    let mut items = Vec::new();
    walk(location, "", value, 1, &mut remaining, &mut items)?;
    Ok(CatalogOverview {
        location: location.into(),
        remote,
        verified,
        items,
        truncated: remaining == 0,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_support::{
        Author, TempDir, git as fixture_git, read_json as read, release, state_with, tree_hash,
        write_json,
    };
    use serde_json::json;
    use std::process::Command;

    const SOURCE: &str = "https://example.com/demo1.git";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    /// Write `release` into the local catalog folder `dir` (as `register` does after Git).
    pub(crate) fn save(dir: &Path, release: &Release) -> Release {
        save_release(&dir.join("catalog.json"), release).unwrap()
    }
    fn loc(dir: &Path) -> &str {
        dir.to_str().unwrap()
    }
    /// Relative paths of every file below `dir` (sorted).
    fn files(dir: &Path) -> Vec<String> {
        fn walk(base: &Path, path: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    // ---- versions, IDs, hashes ------------------------------------------------------

    #[test]
    fn version_key_follows_semver_precedence() {
        let v = |text: &str| version_key(text).unwrap();
        assert_eq!(v("0.0.0"), semver::Version::new(0, 0, 0));
        assert_eq!(v("1.10.0"), semver::Version::new(1, 10, 0));
        assert!(v("1.10.0") > v("1.9.0"));
        assert!(v("2.0.0") > v("1.99.99"));
        assert!(v("0.1.10") > v("0.1.9"));
        let max = format!("{0}.{0}.{0}", u64::MAX);
        assert_eq!(v(&max), semver::Version::new(u64::MAX, u64::MAX, u64::MAX));
        // Pre-releases come before their release, ordered by identifiers.
        let ordered = [
            "1.0.0-0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1-x-y",
            "1.0.1",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("1.3.0-beta.1").to_string(), "1.3.0-beta.1");
    }

    #[test]
    fn version_key_rejects_everything_else() {
        for bad in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "v1.0.0",
            "1.0.0+build.5",
            "1.0.0-beta+build",
            "1.0.0-",
            "1.0.0-beta.",
            "1.0.0-.beta",
            "1.0.0-01",
            "1.0.0-beta.01",
            "1.0.0-beta_1",
            "1.0.0-bêta",
            "01.0.0",
            "1.00.0",
            "1.0.01",
            " 1.0.0",
            "1.0.0 ",
            "1..0",
            "-1.0.0",
            "+1.0.0",
            "1.x.0",
            "١.0.0",
            "18446744073709551616.0.0",
        ] {
            assert_eq!(
                version_key(bad).unwrap_err().kind(),
                "invalid_version",
                "{bad:?}"
            );
        }
        for good in [
            "1.0.0-beta",
            "1.0.0-rc.1",
            "1.0.0-0",
            "1.0.0-x-y.7",
            "1.0.0--",
        ] {
            assert!(version_key(good).is_ok(), "{good:?}");
        }
    }

    #[test]
    fn folders_are_namespaced_ids() {
        assert_eq!(folder("dan/demo1").unwrap(), "namespaces/dan/demo1");
        assert_eq!(folder("demo1").unwrap_err().kind(), "invalid_id");
        assert_eq!(folder("../x").unwrap_err().kind(), "invalid_id");
        assert_eq!(folder("a/b/c").unwrap_err().kind(), "invalid_id");
    }

    #[test]
    fn hash_files_matches_the_documented_example_and_walks_subdirectories() {
        let dir = TempDir::new("catalog");
        let dist = dir.join("dist");
        fs::create_dir_all(dist.join("sub/deeper")).unwrap();
        fs::create_dir_all(dist.join("empty")).unwrap();
        fs::write(dist.join("index.js"), "abc").unwrap();
        fs::write(dist.join("sub/deeper/empty.txt"), "").unwrap();
        let hashes = hash_files(&dist).unwrap();
        assert_eq!(
            hashes,
            BTreeMap::from([
                (
                    "index.js".to_owned(),
                    FileDigest {
                        sha256: ABC.into(),
                        size: 3
                    }
                ),
                (
                    "sub/deeper/empty.txt".to_owned(),
                    FileDigest {
                        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                        size: 0
                    }
                ),
            ])
        );
        let empty = dir.join("empty-dist");
        fs::create_dir(&empty).unwrap();
        assert!(hash_files(&empty).unwrap().is_empty());
    }

    #[test]
    fn hash_files_rejects_symlinks_and_missing_dist() {
        let dir = TempDir::new("catalog");
        assert_eq!(
            hash_files(&dir.join("missing")).unwrap_err().kind(),
            "invalid_manifest"
        );
        let dist = dir.join("dist");
        fs::create_dir(&dist).unwrap();
        fs::write(dir.join("outside.txt"), "x").unwrap();
        std::os::unix::fs::symlink(dir.join("outside.txt"), dist.join("link")).unwrap();
        assert_eq!(hash_files(&dist).unwrap_err().kind(), "unsupported");
        // A symlinked dist directory itself is rejected too.
        fs::remove_file(dist.join("link")).unwrap();
        let link = dir.join("dist-link");
        std::os::unix::fs::symlink(&dist, &link).unwrap();
        assert_eq!(hash_files(&link).unwrap_err().kind(), "unsupported");
    }

    #[test]
    fn validate_release_checks_identity_commit_paths_and_build() {
        let good = release("dan/demo1", "1.0.0", SOURCE, 'a');
        assert!(validate_release(&good, "dan/demo1", "1.0.0").is_ok());
        let mut sha256 = good.clone();
        sha256.commit = "b".repeat(64);
        assert!(validate_release(&sha256, "dan/demo1", "1.0.0").is_ok());
        let mut with_build = good.clone();
        with_build.build = Some(Build {
            url: "https://pkg.example/demo1.tar.zst".into(),
            sha256: ABC.to_uppercase(),
        });
        assert!(validate_release(&with_build, "dan/demo1", "1.0.0").is_ok());
        let mut no_files = good.clone();
        no_files.files.clear();
        assert_eq!(
            validate_release(&no_files, "dan/demo1", "1.0.0")
                .unwrap_err()
                .to_string(),
            "catalog release of dan/demo1 has no file hashes"
        );

        assert_eq!(
            validate_release(&good, "dan/other", "1.0.0")
                .unwrap_err()
                .kind(),
            "catalog"
        );
        assert_eq!(
            validate_release(&good, "dan/demo1", "1.0.1")
                .unwrap_err()
                .kind(),
            "catalog"
        );
        assert_eq!(
            validate_release(&good, "dan/demo1", "1.0")
                .unwrap_err()
                .kind(),
            "invalid_version"
        );
        assert_eq!(
            validate_release(&good, "../x", "1.0.0").unwrap_err().kind(),
            "invalid_id"
        );

        type Change = Box<dyn Fn(&mut Release)>;
        let file = |name: &'static str| -> Change {
            Box::new(move |r: &mut Release| {
                r.files.insert(
                    name.into(),
                    FileDigest {
                        sha256: ABC.into(),
                        size: 1,
                    },
                );
            })
        };
        let build = |url: &'static str, sha: &'static str| -> Change {
            Box::new(move |r: &mut Release| {
                r.build = Some(Build {
                    url: url.into(),
                    sha256: sha.into(),
                })
            })
        };
        let cases: Vec<(&str, Change)> = vec![
            (
                "short commit",
                Box::new(|r: &mut Release| r.commit = "a".repeat(39)),
            ),
            (
                "non-hex commit",
                Box::new(|r: &mut Release| r.commit = "g".repeat(40)),
            ),
            ("empty file name", file("")),
            ("absolute file", file("/etc/passwd")),
            ("parent file", file("../x.js")),
            ("nested parent", file("a/../../x.js")),
            ("dot part", file("./x.js")),
            ("empty part", file("a//x.js")),
            ("trailing slash", file("a/")),
            ("backslash", file("a\\x.js")),
            (
                "bad file hash",
                Box::new(|r: &mut Release| {
                    r.files.get_mut("index.js").unwrap().sha256 = "abc".into();
                }),
            ),
            ("http build", build("http://pkg.example/x", ABC)),
            ("bare https", build("https://", ABC)),
            ("space in url", build("https://pkg.example/a b", ABC)),
            ("file build", build("file:///x", ABC)),
            ("bad build hash", build("https://pkg.example/x", "zz")),
        ];
        for (name, change) in cases {
            let mut r = good.clone();
            change(&mut r);
            let error = validate_release(&r, "dan/demo1", "1.0.0").unwrap_err();
            assert_eq!(error.kind(), "catalog", "{name}");
        }
    }

    #[test]
    fn release_json_omits_empty_files_and_missing_build() {
        let mut r = release("dan/demo1", "0.1.2", "git@github.com:x/demo1.git", '3');
        r.files.clear();
        let value = serde_json::to_value(&r).unwrap();
        assert_eq!(
            value,
            json!({
                "manifest": {"id": "dan/demo1", "version": "0.1.2", "dependencies": {}},
                "source": "git@github.com:x/demo1.git",
                "commit": "3".repeat(40),
                "reference": "dan/demo1@0.1.2"
            })
        );
        let back: Release = serde_json::from_value(value).unwrap();
        assert!(back.files.is_empty() && back.build.is_none());

        let full = json!({
            "manifest": {"id": "dan/demo1", "version": "0.1.0", "dependencies": {}},
            "source": SOURCE, "commit": "a".repeat(40), "reference": "dan/demo1@0.1.0",
            "files": {"index.js": {"sha256": ABC, "size": 3}},
            "build": {"url": "https://pkg.example/x.tar.zst", "sha256": ABC}
        });
        let r: Release = serde_json::from_value(full.clone()).unwrap();
        assert_eq!(serde_json::to_value(&r).unwrap(), full);

        let mut extra = full.clone();
        extra["files"]["index.js"]["mode"] = json!(644);
        assert!(serde_json::from_value::<Release>(extra).is_err());
        let mut extra = full;
        extra["build"]["size"] = json!(1);
        assert!(serde_json::from_value::<Release>(extra).is_err());
    }

    #[test]
    fn locations_are_local_paths_or_http_urls() {
        assert!(is_local("catalog-local").unwrap());
        assert!(is_local("/abs/catalog.json").unwrap());
        assert!(!is_local("https://pkg.example").unwrap());
        for loopback in [
            "http://localhost:8000/catalog.json",
            "http://127.0.0.1:8000",
            "http://[::1]:8000/",
        ] {
            assert!(!is_local(loopback).unwrap(), "{loopback}");
        }
        for insecure in [
            "http://pkg.example/catalog.json",
            "http://localhost.evil.example/",
            "http://localhost@evil.example/",
            "http://127.0.0.1@evil.example/",
            "http://127.evil.example/",
        ] {
            assert_eq!(
                is_local(insecure).unwrap_err().kind(),
                "invalid_argument",
                "{insecure}"
            );
        }
        for bad in ["ftp://pkg.example", "file:///tmp/catalog", "s3://bucket"] {
            assert_eq!(
                is_local(bad).unwrap_err().kind(),
                "invalid_argument",
                "{bad}"
            );
        }
        assert_eq!(
            local_file("dir").unwrap().unwrap(),
            PathBuf::from("dir/catalog.json")
        );
        assert_eq!(
            local_file("dir/other.json").unwrap().unwrap(),
            PathBuf::from("dir/other.json")
        );
    }

    // ---- v2 layout ------------------------------------------------------------------

    #[test]
    fn v2_layout_links_directories_bottom_up() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let stored = save(&cat, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        assert_eq!(stored.manifest.id, "dan/demo1");
        assert_eq!(
            files(&cat),
            [
                "catalog.json",
                "namespaces/catalog.json",
                "namespaces/dan/catalog.json",
                "namespaces/dan/demo1/plugin.json",
                "namespaces/dan/demo1/versions/0.1.0/release.json",
            ]
        );
        let dirs = |p: &str| read(&cat.join(p));
        assert_eq!(
            dirs("catalog.json"),
            json!({"schema": 1, "directories": {"namespaces": "namespaces/"}})
        );
        assert_eq!(
            dirs("namespaces/catalog.json"),
            json!({"schema": 1, "directories": {"dan": "dan/"}})
        );
        assert_eq!(
            dirs("namespaces/dan/catalog.json"),
            json!({"schema": 1, "directories": {"demo1": "demo1/"}})
        );
        assert_eq!(
            dirs("namespaces/dan/demo1/plugin.json"),
            json!({"schema": 1, "id": "dan/demo1", "source": SOURCE,
                   "versions": {"0.1.0": "versions/0.1.0/release.json"},
                   "latest": {"version": "0.1.0", "title": null, "description": null,
                              "provides": [], "tags": []}})
        );
        assert_eq!(
            dirs("namespaces/dan/demo1/versions/0.1.0/release.json"),
            serde_json::to_value(&stored).unwrap()
        );
        let text = fs::read_to_string(cat.join("catalog.json")).unwrap();
        assert!(text.ends_with("}\n"), "pretty JSON with newline");

        save(&cat, &release("dan/demo1", "0.2.0", SOURCE, 'b'));
        save(
            &cat,
            &release("dan/demo2", "1.0.0", "https://example.com/demo2.git", 'c'),
        );
        assert_eq!(
            dirs("catalog.json"),
            json!({"schema": 1, "directories": {"namespaces": "namespaces/"}})
        );
        assert_eq!(
            dirs("namespaces/dan/catalog.json"),
            json!({"schema": 1, "directories": {"demo1": "demo1/", "demo2": "demo2/"}})
        );
        assert_eq!(
            dirs("namespaces/dan/demo1/plugin.json")["versions"],
            json!({"0.1.0": "versions/0.1.0/release.json", "0.2.0": "versions/0.2.0/release.json"})
        );
        // No lock or temporary files remain.
        assert!(
            files(&cat).iter().all(|f| f.ends_with(".json")),
            "{:?}",
            files(&cat)
        );
    }

    #[test]
    fn published_versions_are_immutable() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let original = release("dan/demo1", "0.1.0", SOURCE, 'a');
        save(&cat, &original);
        let before = tree_hash(&cat);
        // Identical retry and an alternate ref to the same commit are no-ops.
        save(&cat, &original);
        let mut alias = original.clone();
        alias.reference = "a".repeat(40);
        let stored = save(&cat, &alias);
        assert_eq!(stored.reference, "dan/demo1@0.1.0");
        assert_eq!(tree_hash(&cat), before);

        let path = cat.join("catalog.json");
        let mut moved = original.clone();
        moved.commit = "b".repeat(40);
        let mut rebuilt = original.clone();
        rebuilt.files.get_mut("index.js").unwrap().size = 4;
        let mut with_build = original.clone();
        with_build.build = Some(Build {
            url: "https://pkg.example/x".into(),
            sha256: ABC.into(),
        });
        for changed in [moved, rebuilt, with_build] {
            let error = save_release(&path, &changed).unwrap_err();
            assert_eq!(error.kind(), "version_conflict");
            assert_eq!(
                error.to_string(),
                "version dan/demo1@0.1.0 already exists and is immutable"
            );
        }
        let mut other_source = release("dan/demo1", "0.2.0", "https://elsewhere/x.git", 'c');
        let error = save_release(&path, &other_source).unwrap_err();
        assert_eq!(error.kind(), "catalog");
        assert!(error.to_string().contains("different source"));
        other_source.source = SOURCE.into();
        other_source.manifest.version = "0.2".into();
        assert_eq!(
            save_release(&path, &other_source).unwrap_err().kind(),
            "invalid_version"
        );
        assert_eq!(tree_hash(&cat), before, "rejected writes change nothing");
    }

    #[test]
    fn release_without_hashes_is_refused() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let mut bare = release("dan/demo1", "0.1.0", SOURCE, 'a');
        bare.files.clear();
        let error = save_release(&cat.join("catalog.json"), &bare).unwrap_err();
        assert_eq!(error.kind(), "catalog", "{error}");
        assert!(!cat.join("namespaces").exists(), "nothing written");
        // A published record without `files` is refused when it is read.
        save(&cat, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        let stored = cat.join("namespaces/dan/demo1/versions/0.1.0/release.json");
        let mut value = read(&stored);
        value.as_object_mut().unwrap().remove("files");
        write_json(&stored, &value);
        let error = lookup(loc(&cat), "dan/demo1", Some("0.1.0")).unwrap_err();
        assert_eq!(error.kind(), "catalog");
        assert!(error.to_string().contains("no file hashes"), "{error}");
    }

    #[test]
    fn catalog_lock_blocks_writers_and_is_released_after_success() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        fs::create_dir_all(&cat).unwrap();
        let lock_path = dir.join(".catalog.catalog.lock");
        let path = cat.join("catalog.json");
        let held = lock(&path).unwrap();
        let error = save_release(&path, &release("dan/demo1", "0.1.0", SOURCE, 'a')).unwrap_err();
        assert_eq!(error.kind(), "locked");
        assert!(files(&cat).is_empty(), "nothing written while locked");
        drop(held);
        save(&cat, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        // A failed write releases the lock too.
        let mut bad = release("dan/demo1", "0.1.0", SOURCE, 'b');
        bad.commit = "c".repeat(40);
        assert!(save_release(&path, &bad).is_err());
        drop(lock(&path).expect("released after a failed write"));
        assert!(lock_path.exists(), "the lock file is kept");
    }

    #[test]
    fn refused_second_catalog_writer_keeps_the_first_lock() {
        let dir = TempDir::new("catalog");
        let path = dir.join("catalog/catalog.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let first = lock(&path).unwrap();
        let error = save_release(&path, &release("dan/demo1", "0.1.0", SOURCE, 'a')).unwrap_err();
        assert_eq!(error.kind(), "locked");

        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "catalog::tests::probe_catalog_lock_helper",
                "--ignored",
            ])
            .env("GITPM_TEST_PROBE_CATALOG", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
            "child acquired the first catalog writer's lock: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        drop(first);
    }

    #[test]
    #[ignore = "helper process for refused_second_catalog_writer_keeps_the_first_lock"]
    fn probe_catalog_lock_helper() {
        let Some(path) = std::env::var_os("GITPM_TEST_PROBE_CATALOG") else {
            return;
        };
        assert_eq!(lock(Path::new(&path)).err().unwrap().kind(), "locked");
    }

    #[test]
    fn stale_catalog_lock_from_a_dead_writer_does_not_block() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        fs::create_dir_all(&cat).unwrap();
        // Left behind by a process that was killed while holding the lock.
        fs::write(dir.join(".catalog.catalog.lock"), "").unwrap();
        save_release(
            &cat.join("catalog.json"),
            &release("dan/demo1", "0.1.0", SOURCE, 'a'),
        )
        .expect("a lock without a live owner must not block forever");
    }

    #[test]
    fn stale_temporary_file_from_a_dead_writer_does_not_block() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        save(&cat, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        fs::write(cat.join("namespaces/dan/demo1/plugin.json.tmp"), "{partial").unwrap();
        save_release(
            &cat.join("catalog.json"),
            &release("dan/demo1", "0.2.0", SOURCE, 'b'),
        )
        .expect("a stale temporary file must not block new releases");
        assert!(!cat.join("namespaces/dan/demo1/plugin.json.tmp").exists());
    }

    // ---- reading --------------------------------------------------------------------

    #[test]
    fn lookup_returns_latest_numeric_or_exact_release() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        for (v, c) in [("1.9.0", 'a'), ("1.10.0", 'b'), ("1.2.0", 'c')] {
            save(&cat, &release("dan/demo1", v, SOURCE, c));
        }
        let latest = lookup(loc(&cat), "dan/demo1", None).unwrap();
        assert_eq!(
            (latest.manifest.version.as_str(), latest.commit),
            ("1.10.0", "b".repeat(40))
        );
        // Also through the JSON file path.
        let file = cat.join("catalog.json");
        assert_eq!(
            lookup(file.to_str().unwrap(), "dan/demo1", Some("1.2.0"))
                .unwrap()
                .commit,
            "c".repeat(40)
        );
        let error = lookup(loc(&cat), "dan/demo1", Some("2.0.0")).unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            (
                "catalog",
                "version not found in catalog: dan/demo1@2.0.0".into()
            )
        );
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", Some("2.0"))
                .unwrap_err()
                .kind(),
            "invalid_version"
        );
        assert_eq!(
            lookup(loc(&cat), "../x", None).unwrap_err().kind(),
            "invalid_id"
        );
        let error = lookup(loc(&cat), "dan/missing", None).unwrap_err();
        assert_eq!(
            (
                error.kind(),
                error.to_string().contains("package not found")
            ),
            ("catalog", true)
        );
        // A local catalog location that does not exist is a caller error, not "no catalog".
        let error = lookup(loc(&dir.join("nowhere")), "dan/demo1", None).unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(error.to_string().ends_with("does not exist"), "{error}");
    }

    #[test]
    fn tampered_v2_files_are_rejected() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        save(&cat, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        let index = cat.join("namespaces/dan/demo1/plugin.json");
        let release_path = cat.join("namespaces/dan/demo1/versions/0.1.0/release.json");
        let original_index = read(&index);
        let original_release = read(&release_path);
        let index_cases = [
            (json!({"schema": 2}), "catalog"),
            (json!({"id": "dan/other"}), "catalog"),
            (
                json!({"versions": {"0.1.0": "../../evil/release.json"}}),
                "catalog",
            ),
            (
                json!({"versions": {"0.1": "versions/0.1/release.json"}}),
                "invalid_version",
            ),
            (json!({"extra": true}), "invalid_data"),
        ];
        for (patch, kind) in index_cases {
            let mut value = original_index.clone();
            for (k, v) in patch.as_object().unwrap() {
                value[k] = v.clone();
            }
            write_json(&index, &value);
            let error = lookup(loc(&cat), "dan/demo1", None).unwrap_err();
            assert_eq!(error.kind(), kind, "{patch}: {error}");
        }
        write_json(&index, &original_index);
        let release_cases = [
            (
                json!({"source": "https://elsewhere/x.git"}),
                "release source mismatch",
            ),
            (json!({"commit": "xyz"}), "invalid catalog commit"),
            (
                json!({"manifest": {"id": "dan/demo1", "version": "0.1.1"}}),
                "catalog identity mismatch",
            ),
        ];
        for (patch, message) in release_cases {
            let mut value = original_release.clone();
            for (k, v) in patch.as_object().unwrap() {
                value[k] = v.clone();
            }
            write_json(&release_path, &value);
            let error = lookup(loc(&cat), "dan/demo1", Some("0.1.0")).unwrap_err();
            assert_eq!(
                (error.kind(), error.to_string()),
                ("catalog", message.into())
            );
        }
        fs::remove_file(&release_path).unwrap();
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", None)
                .unwrap_err()
                .to_string(),
            "release not found"
        );
        fs::write(&release_path, "{not json").unwrap();
        let error = lookup(loc(&cat), "dan/demo1", None).unwrap_err();
        assert!(
            error.to_string().starts_with("invalid catalog JSON"),
            "{error}"
        );
    }

    #[test]
    fn selected_reads_only_latest_and_installed_versions_of_installed_plugins() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        for (v, c) in [("1.9.0", 'a'), ("1.10.0", 'b'), ("1.2.0", 'c')] {
            save(&cat, &release("dan/demo1", v, SOURCE, c));
        }
        save(&cat, &release("dan/other", "1.0.0", SOURCE, 'd'));
        // Another package's broken index is never read.
        fs::write(cat.join("namespaces/dan/other/plugin.json"), "{broken").unwrap();
        // So is a version that is neither latest nor installed.
        fs::write(
            cat.join("namespaces/dan/demo1/versions/1.9.0/release.json"),
            "{broken",
        )
        .unwrap();

        let state = state_with("dan/demo1", SOURCE, "1.2.0", &"c".repeat(40));
        let selected = selected(loc(&cat), &state).unwrap();
        let plugin = &selected.packages["dan/demo1"];
        assert_eq!(plugin.source, SOURCE);
        assert_eq!(
            plugin.versions.keys().collect::<Vec<_>>(),
            ["1.10.0", "1.2.0"]
        );

        // Installed version unknown to the catalog: only the latest.
        let state = state_with("dan/demo1", SOURCE, "1.5.0", &"e".repeat(40));
        let selected_versions = super::selected(loc(&cat), &state).unwrap().packages["dan/demo1"]
            .versions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(selected_versions, ["1.10.0"]);

        // A package absent from a v2 catalog is simply missing.
        let state = state_with("dan/absent", SOURCE, "1.0.0", &"e".repeat(40));
        assert!(
            super::selected(loc(&cat), &state)
                .unwrap()
                .packages
                .is_empty()
        );
    }

    #[test]
    fn latest_skips_pre_releases_and_check_marks_major() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        for (v, c) in [
            ("1.2.0", 'a'),
            ("1.3.0-beta.1", 'b'),
            ("2.0.0", 'c'),
            ("2.1.0-rc.1", 'd'),
        ] {
            save(&cat, &release("dan/demo1", v, SOURCE, c));
        }
        // SemVer order; latest is the newest release; a pre-release is taken by name.
        let summary = browse(loc(&cat)).unwrap();
        let package = summary
            .items
            .iter()
            .find_map(|item| match item {
                OverviewItem::Package { package, .. } => Some(package),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            package.versions,
            ["1.2.0", "1.3.0-beta.1", "2.0.0", "2.1.0-rc.1"]
        );
        assert_eq!(package.latest.as_deref(), Some("2.0.0"));
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", None)
                .unwrap()
                .manifest
                .version,
            "2.0.0"
        );
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", Some("1.3.0-beta.1"))
                .unwrap()
                .commit,
            "b".repeat(40)
        );
        // check: 2.0.0 is a major update; the newer 2.1.0-rc.1 is named, not offered.
        let state = state_with("dan/demo1", SOURCE, "1.2.0", &"a".repeat(40));
        let snapshot = selected(loc(&cat), &state).unwrap();
        let row = crate::check::compare(&state, &snapshot).remove(0);
        assert_eq!(
            (
                row.status,
                row.latest.as_deref(),
                row.major,
                row.prerelease.as_deref()
            ),
            (
                crate::check::CheckStatus::UpdateAvailable,
                Some("2.0.0"),
                true,
                Some("2.1.0-rc.1")
            )
        );
        // An installed pre-release follows its own track to the release.
        let state = state_with("dan/demo1", SOURCE, "1.3.0-beta.1", &"b".repeat(40));
        let row = crate::check::compare(&state, &selected(loc(&cat), &state).unwrap()).remove(0);
        assert_eq!((row.latest.as_deref(), row.major), (Some("2.0.0"), true));
        // Only pre-releases: nothing to install without a version.
        save(&cat, &release("dan/beta", "1.0.0-beta.1", SOURCE, 'e'));
        let error = lookup(loc(&cat), "dan/beta", None).unwrap_err();
        assert_eq!(error.kind(), "catalog");
        assert!(error.to_string().contains("only pre-release"), "{error}");
        assert_eq!(newest_eligible(["1.0.0-beta.1"], None), None);
        assert_eq!(
            newest_eligible(
                ["1.0.0-beta.1", "1.0.0-beta.2", "1.1.0-beta"],
                Some("1.0.0-beta.1")
            ),
            Some("1.0.0-beta.2")
        );
    }

    #[test]
    fn v1_catalog_is_refused() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let v1 = json!({
            "plugins": {
                "dan/demo2": {
                    "source": SOURCE,
                    "versions": {"1.9.0": release("dan/demo2", "1.9.0", SOURCE, 'a')}
                }
            }
        });
        write_json(&cat.join("catalog.json"), &v1);
        let before = tree_hash(&cat);
        let refused = |error: Error| {
            assert_eq!(error.kind(), "catalog", "{error}");
            assert!(error.to_string().contains("v1 catalogs"), "{error}");
        };
        refused(lookup(loc(&cat), "dan/demo2", None).unwrap_err());
        refused(versions(loc(&cat), "dan/demo2").unwrap_err());
        refused(browse(loc(&cat)).unwrap_err());
        let state = state_with("dan/demo2", SOURCE, "1.9.0", &"a".repeat(40));
        refused(selected(loc(&cat), &state).unwrap_err());
        refused(
            save_release(
                &cat.join("catalog.json"),
                &release("dan/demo2", "2.0.0", SOURCE, 'f'),
            )
            .unwrap_err(),
        );
        assert_eq!(tree_hash(&cat), before, "nothing is written");
        assert!(!cat.join(".catalog-v1.json").exists());
    }

    #[test]
    fn existing_root_must_be_a_directory() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let path = cat.join("catalog.json");
        fs::create_dir_all(&cat).unwrap();
        fs::write(&path, "{garbage").unwrap();
        let error = save_release(&path, &release("dan/demo1", "0.1.0", SOURCE, 'a')).unwrap_err();
        assert!(
            error.to_string().starts_with("invalid existing catalog"),
            "{error}"
        );
        assert_eq!(fs::read(&path).unwrap(), b"{garbage");
        write_json(&path, &json!({"schema": 2, "directories": {}}));
        let error = save_release(&path, &release("dan/demo1", "0.1.0", SOURCE, 'a')).unwrap_err();
        assert_eq!(error.kind(), "catalog");
        // An empty folder gets a fresh root.
        let fresh = dir.join("fresh");
        save(&fresh, &release("dan/demo1", "0.1.0", SOURCE, 'a'));
        assert_eq!(
            read(&fresh.join("catalog.json")),
            json!({"schema": 1, "directories": {"namespaces": "namespaces/"}})
        );
    }

    #[test]
    fn index_summary_matches_the_latest_release() {
        let dir = TempDir::new("catalog-summary");
        let cat = dir.join("catalog");
        let index_path = cat.join("namespaces/dan/demo1/plugin.json");
        let check = |version: &str| {
            let index = read(&index_path);
            assert_eq!(index["schema"], 1);
            let stored = lookup(loc(&cat), "dan/demo1", Some(version)).unwrap();
            let metadata = stored.metadata();
            assert_eq!(
                index["latest"],
                json!({
                    "version": version, "title": metadata.title,
                    "description": metadata.description, "provides": metadata.provides,
                    "tags": metadata.tags,
                })
            );
        };
        for version in [
            "3.0.0-beta.2",
            "3.0.0-beta.10",
            "1.10.0",
            "1.9.0",
            "4.0.0-alpha",
        ] {
            let mut record = release("dan/demo1", version, SOURCE, 'a');
            record.manifest.extra = serde_json::from_value(json!({
                "title": version, "description": "Details", "provides": "theme",
                "tags": ["ui", "colors"],
            }))
            .unwrap();
            save(&cat, &record);
            check(if version.starts_with("3.") {
                version
            } else {
                "1.10.0"
            });
        }
        // Re-registering repairs a missing summary without changing version links.
        let mut index = read(&index_path);
        index.as_object_mut().unwrap().remove("latest");
        write_json(&index_path, &index);
        let old = lookup(loc(&cat), "dan/demo1", Some("1.9.0")).unwrap();
        save(&cat, &old);
        check("1.10.0");
    }

    #[test]
    fn browse_reads_only_indexes() {
        let dir = TempDir::new("catalog-summary");
        let cat = dir.join("catalog");
        for name in ["a", "b"] {
            let mut record = release(&format!("dan/{name}"), "1.0.0", SOURCE, 'a');
            record.manifest.extra.insert("title".into(), json!(name));
            save(&cat, &record);
            fs::remove_file(cat.join(format!("namespaces/dan/{name}/versions/1.0.0/release.json")))
                .unwrap();
        }
        let rows = || {
            browse(loc(&cat))
                .unwrap()
                .items
                .into_iter()
                .filter_map(|item| {
                    if let OverviewItem::Package { package, .. } = item {
                        Some(package)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows()
                .iter()
                .map(|p| p.title.as_deref())
                .collect::<Vec<_>>(),
            [Some("a"), Some("b")]
        );
        let path = cat.join("namespaces/dan/a/plugin.json");
        let original = read(&path);
        for bad in [
            serde_json::Value::Null,
            json!({"version": "1.0.0", "provides": false}),
            json!({"version": "9.0.0", "title": "Wrong", "description": null, "provides": [], "tags": []}),
        ] {
            let mut index = original.clone();
            index["latest"] = bad;
            write_json(&path, &index);
            let rows = rows();
            assert_eq!(rows[0].title, None);
            assert_eq!(rows[0].latest.as_deref(), Some("1.0.0"));
            assert_eq!(rows[1].title.as_deref(), Some("b"));
        }
    }

    // ---- browse ---------------------------------------------------------------------

    #[test]
    fn browse_walks_v2_tree_depth_first() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let items = |cat: &Path| browse(loc(cat)).unwrap().items;
        write_json(
            &cat.join("catalog.json"),
            &json!({"schema": 1, "directories": {}}),
        );
        assert!(matches!(
            items(&cat)[..],
            [OverviewItem::Empty { depth: 1 }]
        ));
        save(&cat, &release("dan/demo1", "1.10.0", SOURCE, 'a'));
        save(&cat, &release("dan/demo1", "1.9.0", SOURCE, 'b'));
        save(&cat, &release("acme/demo2", "0.1.0", SOURCE, 'c'));
        let overview = browse(loc(&cat)).unwrap();
        assert_eq!(overview.location, loc(&cat));
        assert!(!overview.remote && !overview.truncated);
        let rows: Vec<String> = overview
            .items
            .iter()
            .map(|item| match item {
                OverviewItem::Directory { depth, name } => format!("{depth} dir {name}"),
                OverviewItem::Empty { depth } => format!("{depth} empty"),
                OverviewItem::Package { depth, package } => format!(
                    "{depth} package {} {:?} {:?}",
                    package.id, package.versions, package.latest
                ),
            })
            .collect();
        assert_eq!(
            rows,
            [
                "1 dir namespaces",
                "2 dir acme",
                "3 dir demo2",
                r#"3 package acme/demo2 ["0.1.0"] Some("0.1.0")"#,
                "2 dir dan",
                "3 dir demo1",
                r#"3 package dan/demo1 ["1.9.0", "1.10.0"] Some("1.10.0")"#,
            ]
        );
    }

    #[test]
    fn browse_follows_only_valid_links_and_is_bounded() {
        let dir = TempDir::new("catalog");
        let cat = dir.join("catalog");
        let root = cat.join("catalog.json");
        let error_of = |value: serde_json::Value| {
            write_json(&root, &value);
            browse(loc(&cat)).unwrap_err()
        };
        let error = error_of(json!({"schema": 1, "directories": {"up": "../"}}));
        assert_eq!(
            (error.kind(), error.to_string()),
            ("catalog", "invalid directory link: ../".into())
        );
        assert_eq!(
            error_of(json!({"schema": 1, "directories": {"a b": "a b/"}})).kind(),
            "invalid_id"
        );
        assert_eq!(
            error_of(json!({"schema": 2, "directories": {}})).kind(),
            "catalog"
        );
        assert_eq!(error_of(json!({"schema": 1})).kind(), "catalog");
        let error = error_of(json!({"schema": 1, "directories": {"gone": "gone/"}}));
        assert!(
            error
                .to_string()
                .contains("missing catalog/package index in gone/")
        );
        fs::remove_file(&root).unwrap();
        assert_eq!(
            browse(loc(&cat)).unwrap_err().to_string(),
            "catalog not found"
        );

        // Nesting deeper than 8 levels is refused.
        let mut path = cat.clone();
        write_json(&root, &json!({"schema": 1, "directories": {"a": "a/"}}));
        for _ in 0..9 {
            path = path.join("a");
            write_json(
                &path.join("catalog.json"),
                &json!({"schema": 1, "directories": {"a": "a/"}}),
            );
        }
        let error = browse(loc(&cat)).unwrap_err();
        assert_eq!(error.to_string(), "catalog nesting is too deep");

        // More than OVERVIEW_LIMIT entries are truncated.
        let wide = dir.join("wide");
        let mut directories = serde_json::Map::new();
        for n in 0..OVERVIEW_LIMIT + 5 {
            let name = format!("p{n:02}");
            directories.insert(name.clone(), json!(format!("{name}/")));
            write_json(
                &wide.join(format!("namespaces/dan/{name}/plugin.json")),
                &json!({"schema": 1, "id": format!("dan/{name}"), "source": SOURCE, "versions": {}}),
            );
        }
        write_json(
            &wide.join("catalog.json"),
            &json!({"schema": 1, "directories": {"namespaces": "namespaces/"}}),
        );
        write_json(
            &wide.join("namespaces/catalog.json"),
            &json!({"schema": 1, "directories": {"dan": "dan/"}}),
        );
        write_json(
            &wide.join("namespaces/dan/catalog.json"),
            &json!({"schema": 1, "directories": directories}),
        );
        let overview = browse(loc(&wide)).unwrap();
        assert!(overview.truncated);
        let directories = overview
            .items
            .iter()
            .filter(|i| matches!(i, OverviewItem::Directory { .. }))
            .count();
        assert_eq!(directories, OVERVIEW_LIMIT);
        let package = overview.items.iter().find_map(|i| match i {
            OverviewItem::Package { package, .. } => Some(package),
            _ => None,
        });
        assert_eq!(
            package.unwrap().latest,
            None,
            "a package without versions has no latest"
        );
    }

    #[test]
    fn ensure_publishable_requires_a_local_directory() {
        let dir = TempDir::new("catalog");
        assert_eq!(ensure_publishable(loc(dir.path())).unwrap(), dir.path());
        let file = dir.join("catalog.json");
        fs::write(&file, "{}").unwrap();
        for bad in [
            file.to_str().unwrap(),
            "https://pkg.example",
            "/nonexistent-pm-dir",
        ] {
            assert_eq!(
                ensure_publishable(bad).unwrap_err().kind(),
                "invalid_argument",
                "{bad}"
            );
        }
    }

    // ---- register and update_sources (real Git) -------------------------------------

    #[test]
    fn register_records_the_committed_tree_with_hashes() {
        let dir = TempDir::new("catalog");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let commit = author.head();
        let committed = fs::read(author.path.join("dist/index.js")).unwrap();
        // Uncommitted edits never reach the catalog.
        fs::write(author.path.join("dist/index.js"), "uncommitted").unwrap();
        fs::write(author.path.join("dist/extra.js"), "uncommitted").unwrap();
        let cat = dir.join("catalog");
        let r = register(loc(&cat), author.source(), "dan/demo1@0.1.0").unwrap();
        assert_eq!(r.commit, commit);
        assert_eq!(r.reference, "dan/demo1@0.1.0");
        assert_eq!(r.source, author.source());
        assert_eq!(r.files.keys().collect::<Vec<_>>(), ["index.js"]);
        assert_eq!(r.files["index.js"].size, committed.len() as u64);
        assert!(r.build.is_none());
        assert_eq!(
            serde_json::to_value(lookup(loc(&cat), "dan/demo1", None).unwrap()).unwrap(),
            serde_json::to_value(&r).unwrap()
        );
        // Same commit through another ref: accepted, the stored reference is kept.
        let again = register(loc(&cat), author.source(), &commit).unwrap();
        assert_eq!(again.reference, "dan/demo1@0.1.0");
    }

    #[test]
    fn register_build_validates_metadata_and_inputs() {
        let dir = TempDir::new("catalog");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let cat = dir.join("catalog");
        let bad = Build {
            url: "http://pkg.example/x.tar.zst".into(),
            sha256: ABC.into(),
        };
        let error = register_build(loc(&cat), author.source(), "HEAD", Some(bad)).unwrap_err();
        assert_eq!(
            (error.kind(), error.to_string()),
            ("catalog", "build URL must use HTTPS".into())
        );
        assert!(!cat.join("namespaces").exists());
        let good = Build {
            url: "https://pkg.example/x.tar.zst".into(),
            sha256: ABC.into(),
        };
        let r = register_build(loc(&cat), author.source(), "HEAD", Some(good)).unwrap();
        assert_eq!(r.build.unwrap().url, "https://pkg.example/x.tar.zst");

        for (url, source, reference, kind) in [
            (
                "https://pkg.example",
                author.source(),
                "HEAD",
                "invalid_argument",
            ),
            (loc(&cat), "-uevil", "HEAD", "invalid_argument"),
            (loc(&cat), author.source(), "--output=x", "invalid_argument"),
            (loc(&cat), author.source(), "missing-ref", "git"),
            (
                loc(&cat),
                dir.join("missing").to_str().unwrap(),
                "HEAD",
                "git",
            ),
        ] {
            let error = register(url, source, reference).unwrap_err();
            assert_eq!(error.kind(), kind, "{url} {source} {reference}");
        }
        // A non-numeric manifest version is refused at registration.
        crate::test_support::write_manifest(
            &author.path,
            &crate::test_support::manifest("dan/demo1", "1.0"),
        );
        fixture_git(&author.path, &["commit", "-q", "-am", "bad version"]);
        assert_eq!(
            register(loc(&cat), author.source(), "HEAD")
                .unwrap_err()
                .kind(),
            "invalid_version"
        );
    }

    #[test]
    fn verified_catalog_takes_only_registered_versions() {
        let dir = TempDir::new("catalog");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        let cat = dir.join("catalog");
        register(loc(&cat), author.source(), "dan/demo1@0.1.0").unwrap();
        assert!(!is_verified(loc(&cat)).unwrap());
        assert!(!browse(loc(&cat)).unwrap().verified);
        crate::test_support::verify_catalog(&cat);
        assert!(is_verified(loc(&cat)).unwrap());
        assert!(browse(loc(&cat)).unwrap().verified);
        // register keeps the flag.
        author.release("0.2.0");
        register(loc(&cat), author.source(), "dan/demo1@0.2.0").unwrap();
        assert_eq!(read(&cat.join("catalog.json"))["verified"], json!(true));
        assert_eq!(versions(loc(&cat), "dan/demo1").unwrap().len(), 2);
        // Bulk generation is refused before anything is read or written.
        author.release("0.3.0");
        let list = sources(dir.path(), "repo\n");
        let before = tree_hash(&cat);
        let error = update_sources(loc(&cat), &list, &mut |_| {}).unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert_eq!(
            error.to_string(),
            "a verified catalog takes versions only one by one through register"
        );
        assert_eq!(tree_hash(&cat), before);
        // The ID stays bound to its first source (namespace ownership).
        let other = Author::new(dir.join("other"), "dan/demo1");
        other.release("0.4.0");
        let error = register(loc(&cat), other.source(), "dan/demo1@0.4.0").unwrap_err();
        assert_eq!(error.kind(), "catalog");
        assert!(error.to_string().contains("different source"), "{error}");
        assert_eq!(tree_hash(&cat), before);
        // An unverified catalog still takes catalog-update.
        let open = dir.join("open-catalog");
        let report = update_sources(loc(&open), &list, &mut |_| {}).unwrap();
        assert_eq!(report.releases, 3);
        assert!(!is_verified(loc(&open)).unwrap());
    }

    #[test]
    fn update_sources_skips_and_reports_a_broken_historical_tag() {
        let dir = TempDir::new("catalog");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        // Old tag pointing at a commit whose manifest says another version.
        fixture_git(&author.path, &["tag", "dan/demo1@0.0.9"]);
        author.release("0.2.0");
        let cat = dir.join("catalog");
        let list = sources(dir.path(), "repo\n");
        let report = update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(report.releases, 2);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].source, author.source());
        assert_eq!(report.skipped[0].tag.name, "dan/demo1@0.0.9");
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", None)
                .unwrap()
                .manifest
                .version,
            "0.2.0"
        );
    }

    #[test]
    fn update_sources_and_register_read_every_package_of_a_monorepo() {
        let dir = TempDir::new("catalog");
        let repo = dir.join("mono");
        crate::scaffold::create_package(&repo, "tilr", "store").unwrap();
        crate::scaffold::create_package(&repo, "tilr", "theme").unwrap();
        fs::write(repo.join("packages/store/dist/index.js"), "store 2").unwrap();
        crate::scaffold::commit_package(&repo, "0.2.0", Some("tilr/store")).unwrap();
        let cat = dir.join("catalog");
        let list = sources(dir.path(), "mono\n");
        let report = update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(report.releases, 3);
        let store = lookup(loc(&cat), "tilr/store", None).unwrap();
        assert_eq!(store.manifest.version, "0.2.0");
        assert_eq!(store.path.as_deref(), Some("packages/store"));
        assert_eq!(store.files["index.js"].size, 7);
        let theme = lookup(loc(&cat), "tilr/theme", None).unwrap();
        assert_eq!(theme.path.as_deref(), Some("packages/theme"));
        // Running again changes nothing (releases are immutable and equal).
        update_sources(loc(&cat), &list, &mut |_| {}).unwrap();

        let other = dir.join("other-catalog");
        let source = repo.to_str().unwrap();
        let error = register(loc(&other), source, "main").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        let release = register(loc(&other), source, "tilr/theme@0.1.0").unwrap();
        assert_eq!(release.path.as_deref(), Some("packages/theme"));
        let release =
            register_package(loc(&other), source, "main", Some("tilr/store"), None).unwrap();
        assert_eq!(release.manifest.version, "0.2.0");
    }

    fn sources(dir: &Path, lines: &str) -> PathBuf {
        let path = dir.join("sources.txt");
        fs::write(&path, lines).unwrap();
        path
    }

    #[test]
    fn update_sources_publishes_every_release_tag_once() {
        let dir = TempDir::new("catalog");
        let scoped = Author::new(dir.join("scoped"), "dan/demo1");
        scoped.release("1.10.0");
        let other = Author::new(dir.join("other"), "acme/demo2");
        let cat = dir.join("catalog");
        let list = sources(dir.path(), "# comment\n\n  scoped  \nother\nscoped\n");
        let mut seen = Vec::new();
        let report = update_sources(loc(&cat), &list, &mut |p| match p {
            Progress::ReadingReleases { source } => seen.push(source),
        })
        .unwrap();
        assert_eq!(report.releases, 3);
        assert!(report.skipped.is_empty());
        assert_eq!(report.sources, [other.source(), scoped.source()]);
        assert_eq!(seen, report.sources);
        let latest = lookup(loc(&cat), "dan/demo1", None).unwrap();
        assert_eq!(latest.manifest.version, "1.10.0");
        assert_eq!(latest.reference, "dan/demo1@1.10.0");
        assert_eq!(latest.files.len(), 1);
        assert_eq!(
            lookup(loc(&cat), "acme/demo2", None).unwrap().reference,
            "acme/demo2@0.1.0"
        );
        // A second run changes nothing.
        let before = tree_hash(&cat);
        update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(tree_hash(&cat), before);
    }

    #[test]
    fn summary_generation_preserves_export_warnings() {
        let dir = TempDir::new("catalog-summary");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let entry = format!("160000,{},dist/vendor", author.head());
        fixture_git(
            &author.path,
            &["update-index", "--add", "--cacheinfo", &entry],
        );
        fixture_git(&author.path, &["commit", "-q", "-m", "add gitlink"]);
        fixture_git(&author.path, &["tag", "-f", "dan/demo1@0.1.0"]);
        let cat = dir.join("catalog");
        let list = sources(dir.path(), "author\n");
        let report = update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("dist/vendor is a Git submodule"));
        assert_eq!(
            read(&cat.join("namespaces/dan/demo1/plugin.json"))["latest"]["version"],
            "0.1.0"
        );
    }

    #[test]
    fn failed_summary_regeneration_leaves_catalog_unchanged() {
        let dir = TempDir::new("catalog-summary");
        let first = Author::new(dir.join("first"), "dan/a");
        let second = Author::new(dir.join("second"), "dan/b");
        second.release("0.2.0");
        let cat = dir.join("catalog");
        let list = sources(dir.path(), "first\nsecond\n");
        update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        first.release("0.2.0");
        fixture_git(&second.path, &["tag", "-d", "dan/b@0.2.0"]);
        // The published newest release is no longer part of source discovery.
        fs::write(
            cat.join("namespaces/dan/b/versions/0.2.0/release.json"),
            "{broken",
        )
        .unwrap();
        let before = tree_hash(&cat);
        assert!(update_sources(loc(&cat), &list, &mut |_| {}).is_err());
        assert_eq!(
            tree_hash(&cat),
            before,
            "preflight must finish before any publication"
        );
    }

    #[test]
    fn update_sources_keeps_build_metadata_and_published_records() {
        let dir = TempDir::new("catalog");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let cat = dir.join("catalog");
        let build = Build {
            url: "https://pkg.example/x.tar.zst".into(),
            sha256: ABC.into(),
        };
        register_build(loc(&cat), author.source(), "dan/demo1@0.1.0", Some(build)).unwrap();
        let list = sources(dir.path(), "author\n");
        update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        let stored = lookup(loc(&cat), "dan/demo1", Some("0.1.0")).unwrap();
        assert_eq!(
            stored.build.as_ref().unwrap().url,
            "https://pkg.example/x.tar.zst"
        );
        let record = cat.join("namespaces/dan/demo1/versions/0.1.0/release.json");
        let published = fs::read(&record).unwrap();

        // The published tag is moved to another commit with the same version: the record
        // stays, with a warning, and the other versions are published.
        fs::write(author.path.join("dist/index.js"), "rewritten").unwrap();
        fixture_git(&author.path, &["commit", "-q", "-am", "rewrite"]);
        let moved = author.head();
        fixture_git(&author.path, &["tag", "-f", "dan/demo1@0.1.0"]);
        author.release("0.2.0");
        let report = update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(report.releases, 2);
        assert_eq!(
            report.warnings,
            [format!(
                "release dan/demo1@0.1.0: tag now points to {}; the published record is kept",
                &moved[..12]
            )]
        );
        assert_eq!(fs::read(&record).unwrap(), published);
        assert_eq!(
            lookup(loc(&cat), "dan/demo1", None)
                .unwrap()
                .manifest
                .version,
            "0.2.0"
        );

        // The release commit is unchanged but the record's file hashes differ (a manual
        // edit): kept as well.
        fixture_git(
            &author.path,
            &["tag", "-f", "dan/demo1@0.1.0", &stored.commit],
        );
        let mut edited: serde_json::Value = serde_json::from_slice(&published).unwrap();
        let files = edited["files"].as_object_mut().unwrap();
        let first = files.keys().next().unwrap().clone();
        files[&first]["sha256"] = serde_json::Value::String(ABC.into());
        let edited = serde_json::to_vec_pretty(&edited).unwrap();
        fs::write(&record, &edited).unwrap();
        let report = update_sources(loc(&cat), &list, &mut |_| {}).unwrap();
        assert_eq!(
            report.warnings,
            [
                "release dan/demo1@0.1.0 differs from the published record; the published record is kept"
            ]
        );
        assert_eq!(fs::read(&record).unwrap(), edited);
    }

    #[test]
    fn update_sources_rejects_bad_inputs() {
        let dir = TempDir::new("catalog");
        let one = Author::new(dir.join("one"), "dan/demo1");
        Author::new(dir.join("two"), "dan/demo1");
        let cat = dir.join("catalog");
        let run = |lines: &str| {
            let list = sources(dir.path(), lines);
            update_sources(loc(&cat), &list, &mut |_| {}).unwrap_err()
        };
        assert_eq!(run("# nothing\n\n").kind(), "invalid_argument");
        assert_eq!(run("-uevil\n").kind(), "invalid_argument");
        assert_eq!(run("missing-folder\n").kind(), "invalid_argument");
        let error = run("one\ntwo\n");
        assert_eq!(
            (error.kind(), error.to_string()),
            ("catalog", "multiple sources claim package dan/demo1".into())
        );
        assert!(
            !cat.exists(),
            "nothing is written before every source was read"
        );
        let list = sources(dir.path(), "one\n");
        assert_eq!(
            update_sources("https://pkg.example", &list, &mut |_| {})
                .unwrap_err()
                .kind(),
            "invalid_argument"
        );
        assert_eq!(
            update_sources(loc(&cat), &dir.join("no-such-file"), &mut |_| {})
                .unwrap_err()
                .kind(),
            "io"
        );
        // The catalog already has this ID from another source.
        save(&cat, &release("dan/demo1", "0.0.1", SOURCE, 'a'));
        let error = update_sources(loc(&cat), &list, &mut |_| {}).unwrap_err();
        assert_eq!(error.kind(), "catalog");
        assert!(error.to_string().contains("different source"));
        let _ = one;
    }
}
