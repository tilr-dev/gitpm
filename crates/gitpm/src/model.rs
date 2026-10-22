//! Persistent formats: package manifest and workspace state. Byte-compatible with the stand.
use crate::error::{Error, Result, ensure};
use crate::{identity, transport::git};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// `plugin.json`. Known keys are validated; any other top-level key is kept in `extra` and
/// written back unchanged (a host can store its own metadata in the same file).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    /// Package-relative committed output folder; absent means `dist`, `.` means root.
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub dist: Option<String>,
    /// Describes how to build the package. gitpm never executes this command.
    #[serde(
        default,
        rename = "buildCommand",
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub build_command: Option<String>,
    /// Missing output should explain that a committed build result is required.
    #[serde(
        default,
        rename = "buildRequired",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub build_required: bool,
    /// Other packages by ID with an npm-style version range (see [`crate::deps::requirement`]).
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
    /// This package can stand in for another one at the declared version. Used only when
    /// the user explicitly replaces that package; never applied automatically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<Replaces>,
    /// Top-level keys this library does not know, kept for round-trips. Written after the
    /// known keys, in their original order (serde_json `preserve_order`).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
// Serde defaults handle absent fields; a present value must be a string, never null.
fn optional_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

impl Manifest {
    /// The selected output folder, relative to the package root.
    pub fn dist_folder(&self) -> &str {
        self.dist.as_deref().unwrap_or("dist")
    }

    pub(crate) fn validate_dist(&self) -> Result<()> {
        let folder = self.dist_folder();
        ensure!(
            folder == "." || crate::packages::validate_path(folder).is_ok(),
            InvalidManifest,
            "invalid manifest dist path: {folder:?}"
        );
        Ok(())
    }

    /// Output tree path relative to the repository root (empty for its root).
    pub(crate) fn dist_path(&self, package: Option<&str>) -> String {
        match (package.filter(|p| !p.is_empty()), self.dist_folder()) {
            (Some(package), ".") => package.to_owned(),
            (None, ".") => String::new(),
            (Some(package), folder) => format!("{package}/{folder}"),
            (None, folder) => folder.to_owned(),
        }
    }

    pub(crate) fn missing_dist(&self, message: impl Into<String>) -> Error {
        let mut message = message.into();
        if self.build_required {
            message.push_str("; this package requires a committed build result");
            if let Some(command) = &self.build_command {
                message.push_str(&format!("; buildCommand: {command}"));
            }
            message.push_str(" (gitpm does not run builds)");
        }
        Error::invalid_manifest(message)
    }

    /// The descriptive metadata of this manifest (`title`, `description`, `provides`,
    /// `tags`), read from the kept top-level keys. These keys stay in [`Manifest::extra`]
    /// so they are written back exactly as they were (order and the string form of
    /// `provides`). Lenient: a key of the wrong type counts as absent (see
    /// [`Manifest::metadata_problems`]), so packages published before 0.5 keep working.
    pub fn metadata(&self) -> Metadata {
        Metadata::read(&self.extra).0
    }
    /// The metadata keys of the wrong type, as messages (`tags must be an array of
    /// strings`); empty when all are valid or absent.
    pub fn metadata_problems(&self) -> Vec<String> {
        Metadata::read(&self.extra).1
    }
    /// [`Manifest::metadata`] for authoring (`init`, `commit`): a key of the wrong type
    /// is an `invalid_manifest` error.
    pub fn metadata_strict(&self) -> Result<Metadata> {
        let (metadata, problems) = Metadata::read(&self.extra);
        match problems.into_iter().next() {
            Some(problem) => Err(Error::invalid_manifest(format!(
                "invalid plugin.json: {problem}"
            ))),
            None => Ok(metadata),
        }
    }
}

/// Descriptive metadata of a package from `plugin.json`. gitpm stores and reports it
/// (state, `list`, catalog) but never interprets it: a host can, for example, group
/// installed packages by what they `provide`. All fields are optional; serialized like
/// everywhere else gitpm reports metadata: an absent string is `null`, an absent list `[]`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Metadata {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Services the package provides; `"provides": "theme"` in `plugin.json` is read as
    /// `["theme"]`.
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}
impl Metadata {
    /// Read the metadata keys of a manifest's top-level object (`null` counts as
    /// absent); a key of the wrong type is left out and reported as a problem.
    fn read(keys: &serde_json::Map<String, serde_json::Value>) -> (Self, Vec<String>) {
        use serde_json::Value;
        let mut problems = Vec::new();
        let mut text = |key: &str| match keys.get(key) {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(_) => {
                problems.push(format!("{key} must be a string"));
                None
            }
        };
        let (title, description) = (text("title"), text("description"));
        let mut list = |key: &str, single: bool| {
            let valid = match keys.get(key) {
                None | Some(Value::Null) => Some(Vec::new()),
                Some(Value::String(text)) if single => Some(vec![text.clone()]),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| item.as_str().map(str::to_owned))
                    .collect(),
                Some(_) => None,
            };
            valid.unwrap_or_else(|| {
                problems.push(format!(
                    "{key} must be {}an array of strings",
                    if single { "a string or " } else { "" }
                ));
                Vec::new()
            })
        };
        let (provides, tags) = (list("provides", true), list("tags", false));
        let metadata = Self {
            title,
            description,
            provides,
            tags,
        };
        (metadata, problems)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Replaces {
    pub id: String,
    pub version: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForkOf {
    pub id: String,
    pub version: String,
    pub commit: String,
    /// Recorded source of the original, so `check` can read its releases without a
    /// catalog. Optional when reading; never empty when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Revision {
    pub version: String,
    pub commit: String,
    /// Only in `previous`: the mode the package had at that revision (omitted in
    /// release mode), so `rollback` restores revision and mode together. Everywhere
    /// else (`current`, results) it is release; the current mode is the record's `mode`.
    #[serde(default, skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
}
impl Revision {
    /// This revision as the `previous` of a record in `mode`.
    pub(crate) fn with_mode(&self, mode: &Mode) -> Self {
        Self {
            mode: mode.clone(),
            ..self.clone()
        }
    }
    /// This revision without a mode (as `current` and in results).
    pub(crate) fn without_mode(&self) -> Self {
        self.with_mode(&Mode::Release)
    }
}
/// What `update` follows for an installed package, recorded in state as `mode`
/// (omitted in release mode, else `{"pinned": REF}` or `{"branch": NAME}`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The newest release (`vVERSION` tag); the default.
    #[default]
    Release,
    /// Fixed at a tag or commit: the ref as the user gave it (a commit in full).
    /// `current.commit` is the installed truth; the ref is for display and `check`.
    Pinned(String),
    /// Follows the HEAD of this branch of the origin.
    Branch(String),
}
impl Mode {
    pub fn is_release(&self) -> bool {
        matches!(self, Mode::Release)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Installed {
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    /// What `update` follows; omitted in release mode.
    #[serde(default, skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
    #[serde(default, rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
    /// Dependency edges of the current revision (copied from its manifest).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<Replaces>,
    /// Catalog the package was installed from (canonical location); `None` for Git installs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    /// Cache key of the shared clone `repos/<clone>` of a multi-package repository (one
    /// per source, used by every package installed from it); `None`: the package's own
    /// clone `packages/<id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<String>,
    /// Package folder inside the repository (from `gitpm.json`); `None`: the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Descriptive metadata of the current revision's manifest. Required in format 1: a
    /// record without it is `invalid_data`.
    pub metadata: Metadata,
}
impl Installed {
    /// Take the manifest-derived fields of a new current revision.
    pub(crate) fn adopt(&mut self, m: &Manifest) {
        self.fork_of = m.fork_of.clone();
        self.dependencies = m.dependencies.clone();
        self.replaces = m.replaces.clone();
        self.metadata = m.metadata();
    }
    /// The root package of its repository (no `gitpm.json` at the installed revision):
    /// `path` is `None`. Independent of where the clone lives: a root package that moved
    /// into a package folder keeps its own clone `packages/<id>` but is no longer a root
    /// package.
    pub fn is_root_package(&self) -> bool {
        self.path.is_none()
    }
    /// Workspace-relative folder of the clone: `repos/<clone>` (shared) or `packages/<id>`.
    pub fn clone_folder(&self, id: &str) -> String {
        match &self.clone {
            Some(key) => format!("repos/{key}"),
            None => format!("packages/{id}"),
        }
    }
}

/// Version of the `state.json` layout this library reads and writes. Stays 1 before
/// the first public release. Later, bump once per release only when the previous public
/// release would misread a change; any other format is refused without migration.
pub const STATE_FORMAT: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct State {
    /// Layout version: always [`STATE_FORMAT`].
    pub format: u32,
    pub packages: BTreeMap<String, Installed>,
    /// Global replacements: original ID → installed package that provides it for every
    /// consumer in this workspace.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub replacements: BTreeMap<String, String>,
    /// Forgotten packages (own or shared clone), with their whole record: the clone is
    /// kept (their `dist/` is not: they are not installed), a shared clone still counts
    /// them as users, and `restore` records them again as they were.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub forgotten: BTreeMap<String, Installed>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            format: STATE_FORMAT,
            packages: BTreeMap::new(),
            replacements: BTreeMap::new(),
            forgotten: BTreeMap::new(),
        }
    }
}
impl State {
    /// `dist/<id>` is deployed exactly for the active packages: installed, enabled and
    /// not replaced by another package. A host loads everything under `dist/`.
    pub fn is_active(&self, id: &str) -> bool {
        self.packages.get(id).is_some_and(|p| p.enabled) && !self.replacements.contains_key(id)
    }
    /// Refuse a `state.json` whose `format` is missing or not [`STATE_FORMAT`] with
    /// `unsupported` (checked before any other field, so an older layout is never
    /// misread). Malformed JSON is `invalid_data`.
    pub(crate) fn check_format(bytes: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        struct Format {
            format: Option<serde_json::Value>,
        }
        let found: Format = serde_json::from_slice(bytes)
            .map_err(|e| Error::InvalidData(format!("invalid workspace state.json: {e}")))?;
        let found = match found.format {
            Some(serde_json::Value::Number(n)) if n.as_u64() == Some(STATE_FORMAT.into()) => {
                return Ok(());
            }
            Some(value) => format!("format {value}"),
            None => "no format".to_owned(),
        };
        Err(Error::Unsupported(format!(
            "state.json has {found}, but this gitpm reads only format {STATE_FORMAT}; re-create the workspace (install its packages again) or use the gitpm that wrote it"
        )))
    }
    /// Parse `state.json` and check what serde cannot: the format version, IDs, clone
    /// keys and package folders.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        Self::check_format(bytes)?;
        let state: State = serde_json::from_slice(bytes)
            .map_err(|e| Error::InvalidData(format!("invalid workspace state.json: {e}")))?;
        for (id, p) in state.packages.iter().chain(&state.forgotten) {
            valid_id(id)?;
            if let Some(key) = &p.clone {
                valid_clone_key(key)?;
            }
            if let Some(path) = &p.path {
                crate::packages::validate_path(path)?;
            }
            if let Mode::Pinned(name) | Mode::Branch(name) = &p.mode {
                ensure!(
                    !name.is_empty() && !name.starts_with('-'),
                    InvalidData,
                    "invalid mode of {id} in state.json: {name:?}"
                );
            }
        }
        // Disabling a replacing package drops its binding, so a binding to a disabled
        // package comes from gitpm <= 0.5 (`disable --force` kept it) or a hand edit.
        for (original, fork) in &state.replacements {
            ensure!(
                state.packages.get(fork).is_none_or(|p| p.enabled),
                InvalidData,
                "state.json replaces {original} by the disabled package {fork} (written by gitpm 0.5 or earlier, or edited by hand); a replacing package is always enabled: remove the entry {original} from replacements"
            );
        }
        for id in state.forgotten.keys() {
            ensure!(
                !state.packages.contains_key(id),
                InvalidData,
                "state.json lists {id} as installed and as forgotten"
            );
        }
        Ok(state)
    }
}

/// A shared clone's cache key (see `repo_key`): a folder name under `repos/` made of
/// ASCII letters, digits, `-` and `_`, ending in a 16-digit hex hash.
pub(crate) fn valid_clone_key(key: &str) -> Result<()> {
    let hash = key.rsplit('-').next().unwrap_or("");
    ensure!(
        key.len() <= 80
            && !key.starts_with('-')
            && key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            && hash.len() == 16
            && hash.bytes().all(|c| c.is_ascii_hexdigit()),
        InvalidData,
        "invalid clone name {key:?} in state.json: expected a folder name under repos/ such as packages-0123456789abcdef"
    );
    Ok(())
}

/// Flat view of one installed package (convenient for UIs and FFI).
#[derive(Clone, Debug, Serialize)]
pub struct InstalledPackage {
    pub id: String,
    pub source: String,
    pub current: Revision,
    pub previous: Option<Revision>,
    pub enabled: bool,
    /// What `update` follows (omitted in release mode).
    #[serde(skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
    /// Installed, enabled and not replaced ([`State::is_active`]): exactly the packages
    /// with `dist/<id>`. `enabled` alone is not enough (a replaced package is enabled).
    pub active: bool,
    #[serde(rename = "forkOf", skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<ForkOf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces: Option<Replaces>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    /// Package folder inside the source repository; `None` for the repository root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Cache key of the shared clone `repos/<clone>` (as in state); `None`: the
    /// package's own clone `packages/<id>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clone: Option<String>,
    /// Descriptive metadata of the installed revision (`plugin.json`); always present
    /// in JSON (`null` / `[]` when absent).
    pub title: Option<String>,
    pub description: Option<String>,
    pub provides: Vec<String>,
    pub tags: Vec<String>,
    /// Each dependency with the package that provides it and whether it is satisfied.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<crate::deps::DependencyStatus>,
    /// Original IDs this package currently stands in for (active replacements).
    #[serde(rename = "replacing", skip_serializing_if = "Vec::is_empty")]
    pub replacing: Vec<String>,
    /// Installed packages that depend on this one (directly, through any replacement).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependents: Vec<String>,
}
impl State {
    /// Flat view of the installed packages (sorted by ID), with the metadata recorded in
    /// state (the clone is never read).
    pub fn installed(&self) -> Vec<InstalledPackage> {
        self.packages
            .iter()
            .map(|(id, p)| (id, p, p.metadata.clone()))
            .map(|(id, p, metadata)| InstalledPackage {
                id: id.clone(),
                source: p.source.clone(),
                current: p.current.clone(),
                previous: p.previous.clone(),
                enabled: p.enabled,
                mode: p.mode.clone(),
                active: self.is_active(id),
                fork_of: p.fork_of.clone(),
                replaces: p.replaces.clone(),
                catalog: p.catalog.clone(),
                path: p.path.clone(),
                clone: p.clone.clone(),
                title: metadata.title,
                description: metadata.description,
                provides: metadata.provides,
                tags: metadata.tags,
                dependencies: crate::deps::status(self, p),
                replacing: self
                    .replacements
                    .iter()
                    .filter(|(_, fork)| *fork == id)
                    .map(|(original, _)| original.clone())
                    .collect(),
                dependents: crate::deps::dependents(self, id),
            })
            .collect()
    }
}

/// `namespace/name`; both components are safe path parts.
pub fn valid_id(id: &str) -> Result<()> {
    let parts: Vec<_> = id.split('/').collect();
    ensure!(
        parts.len() == 2,
        InvalidId,
        "invalid package ID {id:?}: expected namespace/name"
    );
    for part in parts {
        identity::valid_part(part)?;
    }
    Ok(())
}

pub(crate) fn manifest(repo: &Path) -> Result<Manifest> {
    let path = repo.join("plugin.json");
    let bytes = fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::invalid_manifest(format!("missing plugin.json: {}", path.display()))
        }
        _ => Error::Io(format!("cannot read {}: {e}", path.display())),
    })?;
    let m: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| Error::invalid_manifest(format!("invalid plugin.json: {e}")))?;
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
        if let Some(source) = &base.source {
            ensure!(
                !source.trim().is_empty(),
                InvalidManifest,
                "forkOf.source is empty"
            );
            crate::auth::validate_recorded(source)
                .map_err(|e| Error::invalid_manifest(format!("invalid forkOf.source: {e}")))?;
        }
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
    if let Some(replaces) = &m.replaces {
        valid_id(&replaces.id)?;
        ensure!(
            replaces.id != m.id,
            InvalidManifest,
            "a package cannot replace itself"
        );
        crate::catalog::version_key(&replaces.version)?;
    }
    crate::deps::validate(&m)?;
    m.validate_dist()?;
    let folder = m.dist_folder();
    if !repo.join(folder).is_dir() {
        return Err(m.missing_dist(format!("package must contain a ready {folder}/ directory")));
    }
    if folder != "." {
        let mut current = repo.to_path_buf();
        for component in folder.split('/') {
            current.push(component);
            ensure!(
                !fs::symlink_metadata(&current)?.file_type().is_symlink(),
                Unsupported,
                "symlinks are not supported: {}",
                current.display()
            );
        }
    }
    Ok(m)
}

/// Manifest of the package in `path` (`None`: the root) of a checked-out repository.
/// Every folder on the way must be a real directory inside the repository.
pub(crate) fn package_manifest(repo: &Path, path: Option<&str>) -> Result<Manifest> {
    let Some(path) = path else {
        return manifest(repo);
    };
    crate::packages::validate_path(path)?;
    let mut current = repo.to_path_buf();
    for part in path.split('/') {
        current.push(part);
        let meta = fs::symlink_metadata(&current)
            .map_err(|e| Error::invalid_manifest(format!("missing package {path}: {e}")))?;
        ensure!(
            !meta.file_type().is_symlink(),
            Unsupported,
            "symlinks are not supported: {path}"
        );
        ensure!(
            meta.is_dir(),
            InvalidManifest,
            "package {path} is not a directory"
        );
    }
    manifest(&current)
}

pub(crate) fn revision(repo: &Path, m: &Manifest, custom_ssh: bool) -> Result<Revision> {
    Ok(Revision {
        version: m.version.clone(),
        commit: git(repo, &["rev-parse", "HEAD"], custom_ssh)?,
        mode: Mode::Release,
    })
}

pub(crate) fn resolve_ref(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
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
        custom_ssh,
    )
}

/// The name of the origin's default branch (`refs/remotes/origin/HEAD`, set by a clone
/// or by `git remote set-head origin --auto`).
pub(crate) fn default_branch(repo: &Path, custom_ssh: bool) -> Result<String> {
    let name = git(
        repo,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
        custom_ssh,
    )
    .map_err(|_| Error::Git("the origin has no default branch (HEAD)".into()))?;
    let branch = name.strip_prefix("origin/").unwrap_or(&name).to_owned();
    ensure!(
        !branch.is_empty() && !branch.starts_with('-'),
        Git,
        "the origin has no default branch (HEAD)"
    );
    Ok(branch)
}

/// Resolve a user ref against the origin, in a fresh clone or in an installed clone
/// after `fetch`. Only exact names count: a tag (`refs/tags/X`), then an origin branch
/// (`refs/remotes/origin/X`), then a commit ID of 7 to 64 hex digits; `HEAD` (and
/// `refs/remotes/origin/HEAD`) is the origin's default branch, by its name. Revision
/// expressions (`HEAD~1`, `TAG^`) and local branches of a clone (stale copies of the
/// origin) are refused with `invalid_argument`. Returns the commit and the mode that
/// records it: a tag or commit pins, a branch is followed.
pub(crate) fn resolve_upstream_ref(
    repo: &Path,
    reference: &str,
    custom_ssh: bool,
) -> Result<(String, Mode)> {
    ensure!(
        !reference.is_empty() && !reference.starts_with('-'),
        InvalidArgument,
        "invalid git ref"
    );
    let exists =
        |full: &str| git(repo, &["show-ref", "--verify", "--quiet", full], custom_ssh).is_ok();
    let branch = |name: &str| -> Result<(String, Mode)> {
        let commit = resolve_ref(repo, &format!("refs/remotes/origin/{name}"), custom_ssh)?;
        Ok((commit, Mode::Branch(name.to_owned())))
    };
    if reference == "HEAD" || reference == "refs/remotes/origin/HEAD" {
        return branch(&default_branch(repo, custom_ssh)?);
    }
    let unknown = || {
        Error::InvalidArgument(format!(
            "unknown ref {reference}: not a tag, origin branch or commit"
        ))
    };
    if let Some(rest) = reference.strip_prefix("refs/") {
        ensure!(
            !rest.starts_with("heads/"),
            InvalidArgument,
            "local branches of an installed clone are stale; use the branch name"
        );
        if let Some(name) = rest.strip_prefix("remotes/origin/")
            && exists(reference)
        {
            return branch(name);
        }
        if rest.starts_with("tags/") && exists(reference) {
            let commit = resolve_ref(repo, reference, custom_ssh)?;
            return Ok((commit, Mode::Pinned(reference.to_owned())));
        }
        return Err(unknown());
    }
    let tag = format!("refs/tags/{reference}");
    if exists(&tag) {
        return Ok((
            resolve_ref(repo, &tag, custom_ssh)?,
            Mode::Pinned(reference.to_owned()),
        ));
    }
    if exists(&format!("refs/remotes/origin/{reference}")) {
        return branch(reference);
    }
    if (7..=64).contains(&reference.len())
        && reference
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && let Ok(commit) = resolve_ref(repo, reference, custom_ssh)
    {
        return Ok((commit.clone(), Mode::Pinned(commit)));
    }
    Err(unknown())
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
    fn valid_id_accepts_scoped_ids_only() {
        for ok in [
            "dan/demo1",
            "a-b/c_d",
            &format!("{}/{}", "x".repeat(80), "y".repeat(80)),
        ] {
            assert!(valid_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "demo1",
            "all",
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
            "-x/demo1",
            "dan/-demo1",
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
    fn manifest_keeps_unknown_top_level_keys_through_read_and_write() {
        let dir = TempDir::new("model");
        let repo = repo_with(
            &dir,
            r#"{"id":"dan/demo1","version":"0.1.0","name":"Demo","host":{"entry":"index.js","flags":[1,true,null]}}"#,
            true,
        );
        let m = manifest(&repo).unwrap();
        assert_eq!(m.extra["name"], "Demo");
        assert_eq!(m.extra["host"]["entry"], "index.js");
        let text = manifest_json(&m).unwrap();
        let written: serde_json::Value = serde_json::from_str(&text).unwrap();
        let original: serde_json::Value = serde_json::from_str(
            r#"{"id":"dan/demo1","version":"0.1.0","dependencies":{},"name":"Demo","host":{"entry":"index.js","flags":[1,true,null]}}"#,
        )
        .unwrap();
        assert_eq!(written, original);
        // Known keys come first, then the kept ones.
        assert!(
            text.find("\"version\"").unwrap() < text.find("\"host\"").unwrap(),
            "{text}"
        );
        fs::write(repo.join("plugin.json"), &text).unwrap();
        assert_eq!(manifest(&repo).unwrap().extra, m.extra);
    }

    #[test]
    fn manifest_keeps_the_original_order_of_unknown_keys() {
        let dir = TempDir::new("model");
        let repo = repo_with(
            &dir,
            r#"{"zeta":1,"id":"dan/demo1","alpha":{"z":1,"a":2},"version":"0.1.0","mid":true}"#,
            true,
        );
        let text = manifest_json(&manifest(&repo).unwrap()).unwrap();
        let keys: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with("  \"") && !line.starts_with("    "))
            .map(|line| line.trim_start().split('"').nth(1).unwrap())
            .collect();
        assert_eq!(
            keys,
            ["id", "version", "dependencies", "zeta", "alpha", "mid"],
            "{text}"
        );
        // Nested unknown objects keep their order too.
        assert!(
            text.find("\"z\": 1").unwrap() < text.find("\"a\": 2").unwrap(),
            "{text}"
        );
    }

    #[test]
    fn manifest_still_validates_known_keys_next_to_unknown_ones() {
        let dir = TempDir::new("model");
        for (json, kind) in [
            // wrong type for a known key
            (
                r#"{"id":"dan/demo1","version":1,"x":1}"#,
                "invalid_manifest",
            ),
            // invalid known value
            (
                r#"{"id":"dan/demo1","version":"banana","x":1}"#,
                "invalid_version",
            ),
            (r#"{"id":"../x","version":"0.1.0","x":1}"#, "invalid_id"),
            // missing required key
            (r#"{"version":"0.1.0","x":1}"#, "invalid_manifest"),
            // nested known objects stay strict
            (
                r#"{"id":"dan/demo1","version":"0.1.0","replaces":{"id":"dan/other","version":"1.0.0","x":1}}"#,
                "invalid_manifest",
            ),
        ] {
            let _ = fs::remove_dir_all(dir.join("repo"));
            let repo = repo_with(&dir, json, true);
            let error = manifest(&repo).unwrap_err();
            assert_eq!(error.kind(), kind, "{json}: {error}");
        }
    }

    #[test]
    fn manifest_metadata_is_typed_normalized_and_written_back_as_it_was() {
        let dir = TempDir::new("model");
        let json = r#"{"id":"dan/demo1","version":"0.1.0","x":1,"provides":"theme","title":"Demo","tags":["a","b"],"description":null}"#;
        let repo = repo_with(&dir, json, true);
        let m = manifest(&repo).unwrap();
        assert_eq!(
            m.metadata_strict().unwrap(),
            Metadata {
                title: Some("Demo".into()),
                description: None,
                provides: vec!["theme".into()],
                tags: vec!["a".into(), "b".into()],
            }
        );
        // The keys stay where and as they were (string form of provides included).
        let written: serde_json::Value = serde_json::from_str(&manifest_json(&m).unwrap()).unwrap();
        assert_eq!(written["provides"], "theme");
        assert_eq!(written["description"], serde_json::Value::Null);
        let text = manifest_json(&m).unwrap();
        assert!(text.find("\"x\"").unwrap() < text.find("\"provides\"").unwrap());
        let m = Manifest {
            extra: serde_json::from_str(r#"{"provides":["a","b"]}"#).unwrap(),
            ..Default::default()
        };
        assert_eq!(m.metadata().provides, ["a", "b"]);
        assert_eq!(Manifest::default().metadata(), Metadata::default());
        // Wrong types: reading the manifest (install, update, fork) accepts them and the
        // key counts as absent; authoring (`metadata_strict`) refuses them.
        for (bad, problem) in [
            (r#""title":1"#, "title must be a string"),
            (r#""description":["x"]"#, "description must be a string"),
            (
                r#""provides":1"#,
                "provides must be a string or an array of strings",
            ),
            (
                r#""provides":["a",2]"#,
                "provides must be a string or an array of strings",
            ),
            (r#""tags":"a""#, "tags must be an array of strings"),
            (r#""tags":[null]"#, "tags must be an array of strings"),
        ] {
            let _ = fs::remove_dir_all(dir.join("repo"));
            let repo = repo_with(
                &dir,
                &format!(
                    r#"{{"id":"dan/demo1","version":"0.1.0","title":"T","provides":"p",{bad}}}"#
                ),
                true,
            );
            let m = manifest(&repo).unwrap();
            assert_eq!(m.metadata_problems(), [problem], "{bad}");
            let lenient = m.metadata();
            assert!(lenient.tags.is_empty(), "{bad}");
            if !bad.contains("title") {
                assert_eq!(lenient.title.as_deref(), Some("T"), "{bad}");
            }
            let error = m.metadata_strict().unwrap_err();
            assert_eq!(error.kind(), "invalid_manifest", "{bad}: {error}");
            assert!(error.to_string().contains(problem), "{error}");
        }
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
                commit: COMMIT.into(),
                source: None,
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
            (r#"{"id":"dan/demo1"}"#.into(), true, "invalid_manifest"),
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
                r#"{"id":"demo1","version":"0.1.0"}"#.into(),
                true,
                "invalid_id",
            ),
            (
                r#"{"id":"dan/demo1","version":"  "}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"dan/demo1","version":"0.1.0"}"#.into(),
                false,
                "invalid_manifest",
            ),
            (
                r#"{"id":"dan/demo1","version":"0.1.0","dependencies":{"dan/base":"banana"}}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"dan/demo1","version":"0.1.0","dependencies":{"dan/demo1":"^1"}}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"dan/demo1","version":"0.1.0","dependencies":{"../x":"^1"}}"#.into(),
                true,
                "invalid_id",
            ),
            (
                r#"{"id":"me/demo1","version":"0.1.0","replaces":{"id":"me/demo1","version":"0.1.0"}}"#.into(),
                true,
                "invalid_manifest",
            ),
            (
                r#"{"id":"me/demo1","version":"0.1.0","replaces":{"id":"dan/demo1","version":"1.0"}}"#.into(),
                true,
                "invalid_version",
            ),
            (
                r#"{"id":"me/demo1","version":"0.1.0","replaces":{"id":"dan/demo1"}}"#.into(),
                true,
                "invalid_manifest",
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
                    r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}","source":""}}}}"#
                ),
                true,
                "invalid_manifest",
            ),
            (
                format!(
                    r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}","source":"ext::sh -c id"}}}}"#
                ),
                true,
                "invalid_manifest",
            ),
            (
                format!(
                    r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}","source":"fd::5"}}}}"#
                ),
                true,
                "invalid_manifest",
            ),
            (
                format!(
                    r#"{{"id":"me/demo1","version":"0.2.0","forkOf":{{"id":"dan/demo1","version":"0.1.0","commit":"{COMMIT}","source":"foo/bar"}}}}"#
                ),
                true,
                "invalid_manifest",
            ),
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
        // A missing plugin.json is an invalid manifest that names the path.
        let empty = dir.join("empty");
        fs::create_dir_all(empty.join("dist")).unwrap();
        let error = manifest(&empty).unwrap_err();
        assert_eq!(error.kind(), "invalid_manifest");
        assert!(error.to_string().contains("empty/plugin.json"), "{error}");
    }

    #[test]
    fn manifest_rejects_non_numeric_versions() {
        let dir = TempDir::new("model");
        for version in ["1.0", "1.0.0-01", "1.0.0+build", "banana", "01.0.0"] {
            let _ = fs::remove_dir_all(dir.join("repo"));
            let repo = repo_with(
                &dir,
                &format!(r#"{{"id":"dan/demo1","version":"{version}"}}"#),
                true,
            );
            assert!(manifest(&repo).is_err(), "{version} must be rejected");
        }
        let _ = fs::remove_dir_all(dir.join("repo"));
        let repo = repo_with(&dir, r#"{"id":"dan/demo1","version":"1.0.0-beta.1"}"#, true);
        assert_eq!(manifest(&repo).unwrap().version, "1.0.0-beta.1");
    }

    #[test]
    fn manifest_json_is_pretty_with_trailing_newline_and_stable_fields() {
        let mut m = Manifest {
            id: "dan/demo1".into(),
            version: "0.1.0".into(),
            ..Default::default()
        };
        assert_eq!(
            manifest_json(&m).unwrap(),
            "{\n  \"id\": \"dan/demo1\",\n  \"version\": \"0.1.0\",\n  \"dependencies\": {}\n}\n"
        );
        m.fork_of = Some(ForkOf {
            id: "vendor/demo1".into(),
            version: "0.0.9".into(),
            commit: COMMIT.into(),
            source: Some("https://git.example/vendor/demo1.git".into()),
        });
        let text = manifest_json(&m).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["forkOf"]["id"], "vendor/demo1");
        assert!(text.contains("\"forkOf\""), "camelCase key: {text}");
        let back: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.fork_of, m.fork_of);
    }

    /// Byte format of `state.json` (`serde_json::to_vec_pretty`, no newline).
    const STATE: &str = r#"{
  "format": 1,
  "packages": {
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
      "enabled": false,
      "metadata": {
        "title": "Demo",
        "description": null,
        "provides": [
          "theme"
        ],
        "tags": []
      }
    },
    "me/demo1": {
      "source": "/tmp/demo1",
      "current": {
        "version": "1.0.0",
        "commit": "a6e1cb27e482634979ffcfb4f9cdd02ff99005db"
      },
      "previous": null,
      "enabled": true,
      "mode": {
        "pinned": "v1.0.0"
      },
      "forkOf": {
        "id": "vendor/demo1",
        "version": "0.9.0",
        "commit": "0123456789abcdef0123456789abcdef01234567"
      },
      "metadata": {
        "title": null,
        "description": null,
        "provides": [],
        "tags": []
      }
    },
    "me/demo2": {
      "source": "/tmp/demo2",
      "current": {
        "version": "0.3.0",
        "commit": "b6e1cb27e482634979ffcfb4f9cdd02ff99005db"
      },
      "previous": null,
      "enabled": true,
      "mode": {
        "branch": "main"
      },
      "metadata": {
        "title": null,
        "description": null,
        "provides": [],
        "tags": []
      }
    }
  }
}"#;

    #[test]
    fn state_round_trips_byte_for_byte() {
        let state = State::parse(STATE.as_bytes()).unwrap();
        assert_eq!(
            String::from_utf8(serde_json::to_vec_pretty(&state).unwrap()).unwrap(),
            STATE
        );
        let p = &state.packages["dan/demo1"];
        assert!(!p.enabled && p.fork_of.is_none());
        assert_eq!(p.previous.as_ref().unwrap().version, "0.1.0");
        assert_eq!(
            state.packages["me/demo1"].fork_of.as_ref().unwrap().id,
            "vendor/demo1"
        );
        // The recorded modes: omitted for release mode, else pinned or branch.
        assert_eq!(p.mode, Mode::Release);
        assert_eq!(
            state.packages["me/demo1"].mode,
            Mode::Pinned("v1.0.0".into())
        );
        assert_eq!(state.packages["me/demo2"].mode, Mode::Branch("main".into()));
        for bad in [r#"{"pinned": ""}"#, r#"{"branch": "-x"}"#] {
            let text = STATE.replace(
                r#"{
        "branch": "main"
      }"#,
                bad,
            );
            assert_eq!(
                State::parse(text.as_bytes()).unwrap_err().kind(),
                "invalid_data",
                "{bad}"
            );
        }
        // Default state is the file a new workspace starts with.
        assert_eq!(
            serde_json::to_string_pretty(&State::default()).unwrap(),
            "{\n  \"format\": 1,\n  \"packages\": {}\n}"
        );
    }

    #[test]
    fn state_format_other_than_current_is_refused() {
        // gitpm 0.3: no `format`; gitpm 0.4/0.5: format 2 with `plugins`; a newer one.
        let none = STATE
            .replace("  \"format\": 1,\n", "")
            .replace("\"packages\"", "\"plugins\"");
        let two = STATE
            .replace("\"format\": 1", "\"format\": 2")
            .replace("\"packages\"", "\"plugins\"");
        let three = STATE.replace("\"format\": 1", "\"format\": 3");
        let text = STATE.replace("\"format\": 1", "\"format\": \"3\"");
        for (old, found) in [
            (none, "no format"),
            (two, "format 2"),
            (three, "format 3"),
            (text, "format \"3\""),
        ] {
            let bytes = old.clone().into_bytes();
            let error = State::parse(&bytes).unwrap_err();
            assert_eq!(error.kind(), "unsupported", "{old}");
            let message = error.to_string();
            assert!(message.contains(found), "{message}");
            assert!(message.contains("only format 1"), "{message}");
            assert_eq!(bytes, old.as_bytes(), "bytes unchanged");
        }
        // Format 1 with the old key is not a valid state.
        let mixed = STATE.replace("\"packages\"", "\"plugins\"");
        assert_eq!(
            State::parse(mixed.as_bytes()).unwrap_err().kind(),
            "invalid_data"
        );
    }

    #[test]
    fn state_clone_keys_have_their_own_validator() {
        for ok in [
            "mono-0123456789abcdef",
            "0123456789abcdef",
            "a_b-c-0123456789abcdef",
        ] {
            assert!(valid_clone_key(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "mono",
            "../x-0123456789abcdef",
            "mono-0123456789abcde",
            "-0123456789abcdef",
            "a/b-0123456789abcdef",
            ".hidden-0123456789abcdef",
        ] {
            let error = valid_clone_key(bad).unwrap_err();
            assert_eq!(error.kind(), "invalid_data", "{bad:?}");
            assert!(error.to_string().contains("invalid clone name"), "{error}");
        }
        let entry = r#"{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true,"clone":"../evil","path":"packages/x","metadata":{}}"#;
        let text = format!(r#"{{"format":1,"packages":{{"a/b":{entry}}}}}"#);
        assert_eq!(
            State::parse(text.as_bytes()).unwrap_err().kind(),
            "invalid_data"
        );
        // A forgotten record needs a shared clone and must not also be installed.
        let entry = entry.replace("../evil", "x-0123456789abcdef");
        let both =
            format!(r#"{{"format":1,"packages":{{"a/b":{entry}}},"forgotten":{{"a/b":{entry}}}}}"#);
        assert_eq!(
            State::parse(both.as_bytes()).unwrap_err().kind(),
            "invalid_data"
        );
        let ok = format!(r#"{{"format":1,"packages":{{}},"forgotten":{{"a/b":{entry}}}}}"#);
        assert_eq!(State::parse(ok.as_bytes()).unwrap().forgotten.len(), 1);
    }

    #[test]
    fn state_requires_core_fields() {
        for broken in [
            r#"{}"#,
            r#"{"format":1,"packages":{"a":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null}}}"#,
            r#"{"format":1,"packages":{"a":{"current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true}}}"#,
            r#"{"format":1,"packages":{"a":{"source":"s","current":{"version":"1.0.0"},"previous":null,"enabled":true}}}"#,
        ] {
            assert!(serde_json::from_str::<State>(broken).is_err(), "{broken}");
        }
        // forkOf is optional; previous may be null.
        let ok = r#"{"format":1,"packages":{"a":{"source":"s","current":{"version":"1.0.0","commit":"c"},"previous":null,"enabled":true,"metadata":{}}}}"#;
        assert!(serde_json::from_str::<State>(ok).is_ok());
        // metadata is required in format 1: a record without it is invalid data.
        let no_metadata = ok.replace(r#","metadata":{}"#, "");
        assert!(serde_json::from_str::<State>(&no_metadata).is_err());
        let no_metadata = no_metadata.replace(r#""a":"#, r#""dan/a":"#);
        assert_eq!(
            State::parse(no_metadata.as_bytes()).unwrap_err().kind(),
            "invalid_data"
        );
    }

    #[test]
    fn installed_is_a_sorted_flat_view() {
        let state: State = serde_json::from_str(STATE).unwrap();
        let list = state.installed();
        assert_eq!(
            list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["dan/demo1", "me/demo1", "me/demo2"]
        );
        assert_eq!(list[2].mode, Mode::Branch("main".into()));
        assert_eq!(
            serde_json::to_value(&list[2]).unwrap()["mode"]["branch"],
            "main"
        );
        assert!(
            serde_json::to_value(&list[0])
                .unwrap()
                .get("mode")
                .is_none()
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
            let error = resolve_ref(Path::new("/nonexistent-pm-repo"), bad, false).unwrap_err();
            assert_eq!(error.kind(), "invalid_argument", "{bad:?}");
        }
        let dir = TempDir::new("model");
        let author = Author::new(dir.join("author"), "dan/demo1");
        let first = author.head();
        let second = author.commit("0.2.0");
        assert_eq!(resolve_ref(&author.path, "v0.1.0", false).unwrap(), first);
        assert_eq!(
            resolve_ref(&author.path, "refs/tags/v0.1.0", false).unwrap(),
            first
        );
        assert_eq!(resolve_ref(&author.path, "main", false).unwrap(), second);
        assert_eq!(
            resolve_ref(&author.path, &first[..12], false).unwrap(),
            first
        );
        assert_eq!(
            resolve_ref(&author.path, "missing", false)
                .unwrap_err()
                .kind(),
            "git"
        );
        // A tree object is not a commit.
        let tree = fixture_git(&author.path, &["rev-parse", "HEAD^{tree}"]);
        assert_eq!(
            resolve_ref(&author.path, &tree, false).unwrap_err().kind(),
            "git"
        );
        let m = manifest(&author.path).unwrap();
        assert_eq!(
            revision(&author.path, &m, false).unwrap(),
            Revision {
                version: "0.2.0".into(),
                commit: second,
                ..Default::default()
            }
        );
    }
}
