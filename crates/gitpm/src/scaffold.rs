//! Author operations: create an independent package repository, commit releases, set remotes.
//! Never pushes and never touches a workspace.
use crate::catalog::version_key;
use crate::error::{Context, Error, Result, ensure, fail};
use crate::{Manifest, exec::git, identity, model, packages};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn validate_name(name: &str) -> Result<()> {
    identity::valid_part(name)
}

/// `plugin.json` of a new package: ID, version 0.1.0 and the name part of the ID as
/// its `title` (so the metadata keys are visible in the scaffold).
fn new_manifest(id: &str, name: &str) -> Manifest {
    let mut extra = serde_json::Map::new();
    extra.insert("title".into(), name.into());
    Manifest {
        id: id.to_owned(),
        version: "0.1.0".into(),
        extra,
        ..Default::default()
    }
}

/// New single-package repository (the root is the package `id`) with version 0.1.0 at
/// `directory`, which must not exist yet; `release`: also tag it.
fn create_repo(directory: &Path, id: &str, release: bool, custom_ssh: bool) -> Result<PathBuf> {
    crate::valid_id(id)?;
    let name = id.rsplit('/').next().unwrap_or(id);
    if let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let directory = directory.to_path_buf();
    fs::create_dir(&directory).context(
        Error::Workspace,
        format!(
            "cannot create {}; choose a new name if the path already exists",
            directory.display()
        ),
    )?;
    let result = (|| {
        let tag = identity::release_tag(id, "0.1.0");
        let next_tag = identity::release_tag(id, "0.1.1");
        let source = shell_quote(&fs::canonicalize(&directory)?.to_string_lossy());
        let m = new_manifest(id, name);
        fs::write(directory.join("plugin.json"), model::manifest_json(&m)?)?;
        fs::create_dir(directory.join("dist"))?;
        let install = if release {
            format!("gitpm install {source} --ref {tag}")
        } else {
            format!(
                "gitpm commit {source} 0.1.0   # release the first version\ngitpm install {source}"
            )
        };
        fs::write(
            directory.join("dist/index.js"),
            format!(
                "// Ready-to-copy demo artifact. No build step required.\nexport const greeting = \"Hello from {name} 0.1.0\";\n"
            ),
        )?;
        fs::write(directory.join(".gitignore"), ".DS_Store\nnode_modules/\n")?;
        fs::write(
            directory.join("README.md"),
            format!(
                "# {name}\n\nLocal gitpm demo package, version 0.1.0.\n\nEdit `dist/index.js` directly; there is no build step. `plugin.json` holds the\npackage ID and version. This is a demo package fixture.\n\nWith the `gitpm` CLI (the workspace is `--root DIR`, `GITPM_ROOT` or `./gitpm-workspace`):\n\n```sh\n{install}\n# After editing this repository:\ngitpm commit {source} 0.1.1\ngitpm update {id} --ref {next_tag}\ngitpm rollback {id}\n```\n\nTo generate a static catalog: add the repository URL/path to sources.txt, then run\n(`GITPM_CATALOG` can replace `--catalog`):\n\n```sh\ngitpm --catalog ./catalog catalog-update\n```\n\nNo remote is configured. If Git identity was missing, the scaffold configured\nPackage Demo <demo@example.invalid> locally in this repository only.\n"
            ),
        )?;
        git(&directory, &["init", "-b", "main"], custom_ssh)?;
        require_identity(&directory, true, custom_ssh)?;
        git(&directory, &["add", "--all", "--", "."], custom_ssh)?;
        git(
            &directory,
            &["commit", "-m", &format!("Create {name} 0.1.0")],
            custom_ssh,
        )?;
        if release {
            git(&directory, &["tag", "--", &tag], custom_ssh)?;
        }
        Ok(fs::canonicalize(&directory)?)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

/// Shell-safe paths in printed next-step commands and the generated README.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Check both identities without Git's login/host fallback. Only repositories newly
/// created by gitpm may receive a local demo identity, and configured values stay intact.
pub(crate) fn require_identity(repo: &Path, created: bool, custom_ssh: bool) -> Result<()> {
    let configured = || {
        git(
            repo,
            &["-c", "user.useConfigOnly=true", "var", "GIT_AUTHOR_IDENT"],
            custom_ssh,
        )
        .and_then(|_| {
            git(
                repo,
                &[
                    "-c",
                    "user.useConfigOnly=true",
                    "var",
                    "GIT_COMMITTER_IDENT",
                ],
                custom_ssh,
            )
        })
        .is_ok()
    };
    if !configured() {
        if created {
            if git(repo, &["config", "user.name"], custom_ssh).is_err() {
                git(
                    repo,
                    &["config", "--local", "user.name", "Package Demo"],
                    custom_ssh,
                )?;
            }
            if git(repo, &["config", "user.email"], custom_ssh).is_err() {
                git(
                    repo,
                    &["config", "--local", "user.email", "demo@example.invalid"],
                    custom_ssh,
                )?;
            }
        }
        ensure!(
            configured(),
            InvalidArgument,
            "no Git identity configured; set user.name and user.email"
        );
    }
    Ok(())
}

/// Repository root, manifest path and manifest of the package to release.
fn locate(
    repo: &Path,
    package: Option<&str>,
    custom_ssh: bool,
) -> Result<(PathBuf, String, Manifest)> {
    let repo = fs::canonicalize(repo)?;
    ensure!(
        Path::new(&git(&repo, &["rev-parse", "--show-toplevel"], custom_ssh)?) == repo,
        InvalidArgument,
        "pass the repository root"
    );
    if let Some(package) = package {
        crate::valid_id(package)?;
    }
    let folder = match packages::packages_in(&repo)? {
        Some(found) => Some(packages::choose(found, package, "")?.path),
        None => None,
    };
    let manifest_path = match &folder {
        Some(folder) => format!("{folder}/plugin.json"),
        None => "plugin.json".to_owned(),
    };
    let m = model::package_manifest(&repo, folder.as_deref())?;
    if let Some(package) = package {
        ensure!(
            m.id == package,
            InvalidArgument,
            "repository contains package {}, not {package}",
            m.id
        );
    }
    Ok((repo, manifest_path, m))
}

/// Release the version the package's manifest already has: commit ALL changes (if
/// there are any) as `Release ID VERSION` and tag it. For a fresh fork, which keeps
/// the version it started from. Never pushes.
pub fn release_current(repo: &Path, package: Option<&str>) -> Result<String> {
    release_current_with_ssh(repo, package, false)
}

pub(crate) fn release_current_with_ssh(
    repo: &Path,
    package: Option<&str>,
    custom_ssh: bool,
) -> Result<String> {
    let (repo, _, m) = locate(repo, package, custom_ssh)?;
    let tag = identity::release_tag(&m.id, &m.version);
    ensure!(
        git(&repo, &["tag", "--list", "--", &tag], custom_ssh)?.is_empty(),
        VersionConflict,
        "tag already exists: {tag}"
    );
    if !git(&repo, &["status", "--porcelain"], custom_ssh)?.is_empty() {
        git(&repo, &["add", "--all", "--", "."], custom_ssh)?;
        git(
            &repo,
            &["commit", "-m", &format!("Release {} {}", m.id, m.version)],
            custom_ssh,
        )?;
    }
    git(&repo, &["tag", "--", &tag], custom_ssh)?;
    Ok(tag)
}

/// The release [`commit_package`] made: the package, its new version and the tag.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Commit {
    pub package: String,
    pub version: String,
    /// Release tag `ID@VERSION`.
    pub tag: String,
}

/// Commit package changes locally and create an immutable version tag. Never pushes.
pub fn commit(repo: &Path, version: &str) -> Result<Commit> {
    commit_with_ssh(repo, version, false)
}

pub(crate) fn commit_with_ssh(repo: &Path, version: &str, custom_ssh: bool) -> Result<Commit> {
    commit_package_with_ssh(repo, version, None, custom_ssh)
}

/// [`commit`] for one package: in a repository with `gitpm.json`, `package` picks the
/// package by ID (optional when it lists only one) and the tag is `ID@VERSION`. Like
/// [`commit`] it stages ALL changes of the repository, not only the package folder.
/// For a single-package repository a given `package` must match the root manifest.
pub fn commit_package(repo: &Path, version: &str, package: Option<&str>) -> Result<Commit> {
    commit_package_with_ssh(repo, version, package, false)
}

pub(crate) fn commit_package_with_ssh(
    repo: &Path,
    version: &str,
    package: Option<&str>,
    custom_ssh: bool,
) -> Result<Commit> {
    version_key(version)?;
    let (repo, manifest_path, mut m) = locate(repo, package, custom_ssh)?;
    // Authoring is strict about metadata types; installing is lenient.
    m.metadata_strict()?;
    let tag = identity::release_tag(&m.id, version);
    ensure!(
        git(&repo, &["tag", "--list", "--", &tag], custom_ssh)?.is_empty(),
        VersionConflict,
        "tag already exists: {tag}"
    );
    if m.version == version {
        // Retry after a failed tag: the release commit exists, only the tag is missing.
        let head: Manifest = serde_json::from_str(&git(
            &repo,
            &["show", &format!("HEAD:{manifest_path}")],
            custom_ssh,
        )?)
        .map_err(|e| Error::invalid_manifest(format!("invalid {manifest_path} at HEAD: {e}")))?;
        ensure!(
            head.version == version
                && git(&repo, &["status", "--porcelain"], custom_ssh)?.is_empty()
                && released_by_head(&repo, &m.id, version, custom_ssh)?,
            VersionConflict,
            "choose a new version"
        );
        git(&repo, &["tag", "--", &tag], custom_ssh)?;
        return Ok(Commit {
            package: m.id,
            version: version.to_owned(),
            tag,
        });
    }
    require_identity(&repo, false, custom_ssh)?;
    // A failed commit (hook, signing) must leave the manifest and the index as they were.
    let original = fs::read(repo.join(&manifest_path))?;
    let index = PathBuf::from(git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        custom_ssh,
    )?);
    let saved_index = fs::read(&index).ok();
    m.version = version.to_owned();
    fs::write(repo.join(&manifest_path), model::manifest_json(&m)?)?;
    let committed = git(&repo, &["add", "--all", "--", "."], custom_ssh).and_then(|_| {
        git(
            &repo,
            &["commit", "-m", &format!("Release {} {version}", m.id)],
            custom_ssh,
        )
    });
    if let Err(error) = committed {
        fs::write(repo.join(&manifest_path), original)?;
        match saved_index {
            Some(bytes) => fs::write(&index, bytes)?,
            None => {
                let _ = fs::remove_file(&index);
            }
        }
        return Err(Error::Git(format!(
            "commit failed; plugin.json and the index were restored, your edits remain: {error}"
        )));
    }
    git(&repo, &["tag", "--", &tag], custom_ssh).context(
        Error::Git,
        "commit succeeded but tag creation failed; releasing the same version again creates the tag",
    )?;
    Ok(Commit {
        package: m.id,
        version: version.to_owned(),
        tag,
    })
}

/// HEAD is the commit that set `version` of package `id` and only lacks its tag: a
/// `Release ID VERSION` commit whose tag failed, or the initial commit of `init
/// --no-release` (`Create NAME 0.1.0` / `Create ID 0.1.0`).
fn released_by_head(repo: &Path, id: &str, version: &str, custom_ssh: bool) -> Result<bool> {
    let subject = git(repo, &["log", "-1", "--format=%s"], custom_ssh)?;
    let name = id.rsplit('/').next().unwrap_or(id);
    Ok(subject == format!("Release {id} {version}")
        || (version == "0.1.0"
            && (subject == format!("Create {name} 0.1.0")
                || subject == format!("Create {id} 0.1.0"))))
}

/// Folder for a new package `name` in a repository with this config: under the first
/// `PREFIX/*` pattern, else `packages/<name>`.
fn package_folder(config: &packages::Config, name: &str) -> String {
    config
        .packages
        .iter()
        .find_map(|pattern| {
            let prefix = pattern.strip_suffix("/*")?;
            (!prefix.contains('*')).then(|| format!("{prefix}/{name}"))
        })
        .unwrap_or_else(|| format!("packages/{name}"))
}

/// Where [`init`] created a package.
#[derive(Clone, Debug)]
pub struct Init {
    /// The repository (canonical path).
    pub repo: PathBuf,
    /// The package folder inside `repo`; `None`: a new single-package repository whose
    /// root is the package.
    pub folder: Option<String>,
    /// The release tag `ID@0.1.0`; `None` with [`InitOptions::release`] off.
    pub tag: Option<String>,
}

/// Options of [`init_with`].
#[derive(Clone, Copy, Debug)]
pub struct InitOptions {
    /// A `dir` that does not exist (or an empty folder that is not a repository yet)
    /// becomes a multi-package repository (`gitpm.json` with `["packages/*"]`, the
    /// package in `packages/<name>`) instead of a single-package one.
    pub multi: bool,
    /// Tag the initial commit as release `ID@0.1.0` (default). Off: only the commit;
    /// release it later with [`commit_package`] (version 0.1.0 or a new one).
    pub release: bool,
}
impl Default for InitOptions {
    fn default() -> Self {
        Self {
            multi: false,
            release: true,
        }
    }
}

/// Create the package `namespace/name` with release 0.1.0 in the repository `dir`. The
/// repository is always given; it is never derived from the ID:
///
/// - `dir` does not exist: a new single-package repository there (the root is the
///   package, tag `ID@0.1.0`).
/// - `dir` is a repository with `gitpm.json`, or one without it whose root is no
///   package: the package is added as a folder (see [`create_package`]).
/// - `dir` is a single-package repository (root `plugin.json`): refused, as is a folder
///   that is not a Git repository.
pub fn init(dir: &Path, namespace: &str, name: &str) -> Result<Init> {
    init_with_ssh(dir, namespace, name, false)
}

pub(crate) fn init_with_ssh(
    dir: &Path,
    namespace: &str,
    name: &str,
    custom_ssh: bool,
) -> Result<Init> {
    init_with_with_ssh(dir, namespace, name, InitOptions::default(), custom_ssh)
}

/// [`init`] with options: `multi` makes a new (or empty, not yet a repository) `dir` a
/// multi-package repository; `release: false` skips the tag.
pub fn init_with(dir: &Path, namespace: &str, name: &str, options: InitOptions) -> Result<Init> {
    init_with_with_ssh(dir, namespace, name, options, false)
}

pub(crate) fn init_with_with_ssh(
    dir: &Path,
    namespace: &str,
    name: &str,
    options: InitOptions,
    custom_ssh: bool,
) -> Result<Init> {
    let id = identity::qualified(namespace, name)?;
    let tag = options.release.then(|| identity::release_tag(&id, "0.1.0"));
    let missing = fs::symlink_metadata(dir).is_err();
    if missing && !options.multi {
        return Ok(Init {
            repo: create_repo(dir, &id, options.release, custom_ssh)?,
            folder: None,
            tag,
        });
    }
    let empty = !missing
        && dir.is_dir()
        && fs::symlink_metadata(dir.join(".git")).is_err()
        && fs::read_dir(dir)?.next().is_none();
    if !(missing || (empty && options.multi)) {
        ensure!(
            dir.is_dir() && fs::symlink_metadata(dir.join(".git")).is_ok(),
            InvalidArgument,
            "{} exists but is not a Git repository; pass a new folder or a repository{}",
            dir.display(),
            if empty {
                " (an empty folder can become a multi-package repository)"
            } else {
                ""
            }
        );
    }
    let folder = add_package(dir, namespace, name, options.release, empty, custom_ssh)?;
    let repo = fs::canonicalize(dir)?;
    let relative = folder
        .strip_prefix(&repo)
        .ok()
        .and_then(|p| p.to_str())
        .map(str::to_owned);
    Ok(Init {
        repo,
        folder: relative,
        tag,
    })
}

/// Whether `init` into `dir` without `multi` is refused for a reason a multi-package
/// repository solves: `dir` is an empty folder that is not a repository (`multi` makes it
/// one), or a repository whose root is a package without `gitpm.json` (only a new
/// multi-package repository can take more packages). Read-only; for callers that show
/// the option after an `invalid_argument` refusal.
pub fn suggests_multi(dir: &Path) -> bool {
    let is_repo = fs::symlink_metadata(dir.join(".git")).is_ok();
    let empty = dir.is_dir() && fs::read_dir(dir).is_ok_and(|mut e| e.next().is_none());
    (empty && !is_repo)
        || (is_repo && dir.join("plugin.json").is_file() && !dir.join(packages::CONFIG).exists())
}

/// Add the package `namespace/name` with release 0.1.0 (commit + tag `ID@0.1.0`) to the
/// multi-package repository `repo`. A missing `repo` becomes a new repository with
/// `gitpm.json` (`["packages/*"]`). In an existing repository without `gitpm.json` one is
/// created, unless the root already is a package (`plugin.json`): moving it is up to the
/// author. The folder is the first `PREFIX/*` pattern's `PREFIX/<name>` (else
/// `packages/<name>`; a literal entry is added to `gitpm.json` when no pattern matches
/// it). Only `gitpm.json` and the new folder are committed; other changes stay as they
/// are, and uncommitted changes to `gitpm.json` are refused. In an existing repository
/// a missing Git identity is refused (never written); a failed commit restores the
/// repository; a failed tag keeps the commit and the error names the tag to create.
/// Returns the package folder.
pub fn create_package(repo: &Path, namespace: &str, name: &str) -> Result<PathBuf> {
    create_package_with_ssh(repo, namespace, name, false)
}

pub(crate) fn create_package_with_ssh(
    repo: &Path,
    namespace: &str,
    name: &str,
    custom_ssh: bool,
) -> Result<PathBuf> {
    add_package(repo, namespace, name, true, false, custom_ssh)
}

/// [`create_package`]; `release: false` skips the tag; `adopt_empty`: `repo` is an
/// existing empty folder (not a repository) that becomes the new repository.
fn add_package(
    repo: &Path,
    namespace: &str,
    name: &str,
    release: bool,
    adopt_empty: bool,
    custom_ssh: bool,
) -> Result<PathBuf> {
    let id = identity::qualified(namespace, name)?;
    let created = adopt_empty || !repo.exists();
    if created {
        fs::create_dir_all(repo)?;
        git(repo, &["init", "-b", "main"], custom_ssh)?;
    }
    let repo = fs::canonicalize(repo)?;
    let result = (|| {
        ensure!(
            Path::new(&git(&repo, &["rev-parse", "--show-toplevel"], custom_ssh)?) == repo,
            InvalidArgument,
            "pass the repository root"
        );
        if !created {
            // The commit takes gitpm.json as it is on disk: it must be the committed one.
            let unclean = crate::lost_work::changed_paths(&repo, &[packages::CONFIG], custom_ssh)?;
            ensure!(
                unclean.is_empty(),
                InvalidArgument,
                "commit or discard the changes to {} in {} first",
                unclean.join(", "),
                repo.display()
            );
        }
        let config_path = repo.join(packages::CONFIG);
        let original_config = fs::read(&config_path).ok();
        let mut config = match packages::read_config(&repo)? {
            Some(config) => config,
            None => {
                ensure!(
                    !repo.join("plugin.json").exists(),
                    InvalidArgument,
                    "the repository root is a package (plugin.json); a multi-package repository needs {} listing package folders, so move the root package into one first",
                    packages::CONFIG
                );
                packages::Config {
                    packages: vec!["packages/*".into()],
                }
            }
        };
        let existing = packages::packages_in(&repo)?.unwrap_or_default();
        // One rule for IDs within a repository: unique in any letter case.
        if let Some(other) = existing
            .iter()
            .find(|p| p.manifest.id.eq_ignore_ascii_case(&id))
        {
            fail!(
                InvalidArgument,
                "package {} already exists in this repository ({})",
                other.manifest.id,
                other.path
            );
        }
        let tag = identity::release_tag(&id, "0.1.0");
        ensure!(
            git(&repo, &["tag", "--list", "--", &tag], custom_ssh)?.is_empty(),
            VersionConflict,
            "tag already exists: {tag}"
        );
        let folder = package_folder(&config, name);
        if !config
            .packages
            .iter()
            .any(|p| packages::matches(p, &folder))
        {
            config.packages.push(folder.clone());
        }
        let directory = repo.join(&folder);
        if fs::symlink_metadata(&directory).is_ok() {
            // Folders are named after the package name: a/ui and b/ui cannot share one.
            match existing.iter().find(|p| p.path == folder) {
                Some(other) => fail!(
                    InvalidArgument,
                    "cannot create {id} in {folder}: package {} already uses that folder (package folders are named after the name part of the ID, so {} and {id} cannot be in one repository)",
                    other.manifest.id,
                    other.manifest.id
                ),
                None => fail!(
                    InvalidArgument,
                    "folder {folder} already exists in {}",
                    repo.display()
                ),
            }
        }
        // gitpm never writes an identity into a user's repository; a new one is its own.
        require_identity(&repo, created, custom_ssh)?;
        // A failed commit (hook, signing) must leave the index as it was, too.
        let index = PathBuf::from(git(
            &repo,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            custom_ssh,
        )?);
        let saved_index = fs::read(&index).ok();
        // The outermost folder this creates (`packages/` too when it is new).
        let new_folder = directory
            .ancestors()
            .take_while(|p| *p != repo && fs::symlink_metadata(p).is_err())
            .last()
            .unwrap_or(&directory)
            .to_path_buf();
        let written = (|| {
            fs::create_dir_all(directory.join("dist"))?;
            let m = new_manifest(&id, name);
            fs::write(directory.join("plugin.json"), model::manifest_json(&m)?)?;
            fs::write(
                directory.join("dist/index.js"),
                format!(
                    "// Ready-to-copy demo artifact. No build step required.\nexport const greeting = \"Hello from {name} 0.1.0\";\n"
                ),
            )?;
            fs::write(
                &config_path,
                format!("{}\n", serde_json::to_string_pretty(&config)?),
            )?;
            git(&repo, &["add", "--", packages::CONFIG, &folder], custom_ssh)?;
            if created {
                git(
                    &repo,
                    &["commit", "-m", &format!("Create {id} 0.1.0")],
                    custom_ssh,
                )?;
            } else {
                git(
                    &repo,
                    &[
                        "commit",
                        "-m",
                        &format!("Create {id} 0.1.0"),
                        "--",
                        packages::CONFIG,
                        &folder,
                    ],
                    custom_ssh,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = written {
            if !created {
                match &saved_index {
                    Some(bytes) => {
                        let _ = fs::write(&index, bytes);
                    }
                    None => {
                        let _ = git(
                            &repo,
                            &["reset", "-q", "--", packages::CONFIG, &folder],
                            custom_ssh,
                        );
                    }
                }
                let _ = fs::remove_dir_all(&new_folder);
                match &original_config {
                    Some(bytes) => {
                        let _ = fs::write(&config_path, bytes);
                    }
                    None => {
                        let _ = fs::remove_file(&config_path);
                    }
                }
            }
            return Err(error);
        }
        // The commit stays: the working tree is left as it is, only the tag is missing.
        if !release {
            return Ok(directory);
        }
        if let Err(error) = git(&repo, &["tag", "--", &tag], custom_ssh) {
            if created {
                return Err(error);
            }
            fail!(
                Git,
                "{id} 0.1.0 was committed in {} but not tagged ({error}); create the tag {tag} on that commit (git tag {tag}) to release it",
                repo.display()
            );
        }
        Ok(directory)
    })();
    if result.is_err() && created {
        let _ = fs::remove_dir_all(&repo);
        if adopt_empty {
            // The folder was there (empty) before: leave it as it was.
            let _ = fs::create_dir(&repo);
        }
    }
    result
}

/// Set the author's origin for fetch and push; no network operations.
pub fn set_remote(repo: &Path, url: &str) -> Result<()> {
    set_remote_with_ssh(repo, url, false)
}

pub(crate) fn set_remote_with_ssh(repo: &Path, url: &str, custom_ssh: bool) -> Result<()> {
    ensure!(
        !url.trim().is_empty() && !url.starts_with('-') && !url.chars().any(char::is_control),
        InvalidArgument,
        "invalid git remote URL"
    );
    let repo = fs::canonicalize(repo)?;
    let top = fs::canonicalize(git(&repo, &["rev-parse", "--show-toplevel"], custom_ssh)?)?;
    ensure!(
        top == repo,
        InvalidArgument,
        "pass the package repository root"
    );
    ensure!(
        repo.join("plugin.json").is_file() || repo.join(packages::CONFIG).is_file(),
        InvalidManifest,
        "plugin.json not found in repository root"
    );
    let remotes = git(&repo, &["remote"], custom_ssh)?;
    if remotes.lines().any(|name| name == "origin") {
        // Replace all configured URLs so an old secondary destination cannot remain.
        git(
            &repo,
            &[
                "config",
                "--local",
                "--replace-all",
                "remote.origin.url",
                url,
            ],
            custom_ssh,
        )?;
        if git(
            &repo,
            &["config", "--get-all", "remote.origin.pushurl"],
            custom_ssh,
        )
        .is_ok()
        {
            git(
                &repo,
                &[
                    "config",
                    "--local",
                    "--replace-all",
                    "remote.origin.pushurl",
                    url,
                ],
                custom_ssh,
            )?;
        }
    } else {
        git(&repo, &["remote", "add", "--", "origin", url], custom_ssh)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempDir, git as fixture_git, tree_hash};

    // Cargo supplies a global test identity. Run the missing-identity case in a
    // single isolated test process so other parallel unit tests keep their config.
    fn isolated_identity_test(name: &str) -> bool {
        if std::env::var_os("GITPM_IDENTITY_TEST_CHILD").is_some() {
            return false;
        }
        let dir = TempDir::new("identity-test-config");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name])
            .env("GITPM_IDENTITY_TEST_CHILD", "1")
            .env("GIT_CONFIG_GLOBAL", dir.join("missing-config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_AUTHOR_NAME")
            .env_remove("GIT_AUTHOR_EMAIL")
            .env_remove("GIT_COMMITTER_NAME")
            .env_remove("GIT_COMMITTER_EMAIL")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "expected one child test to pass: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        true
    }

    #[test]
    fn shell_quote_survives_single_quotes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), r#"'it'"'"'s'"#);
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    fn new_repo(dir: &TempDir, folder: &str) -> PathBuf {
        init(&dir.join(folder), "dan", "demo1").unwrap().repo
    }

    #[test]
    fn init_into_a_new_folder_makes_a_tagged_single_package_repository() {
        let dir = TempDir::new("scaffold");
        // The folder is given; it is never derived from the ID.
        let created = init(&dir.join("nested/hello"), "dan", "demo1").unwrap();
        assert!(created.folder.is_none());
        let repo = created.repo;
        assert_eq!(repo, dir.join("nested/hello"));
        let m = model::manifest(&repo).unwrap();
        assert_eq!((m.id.as_str(), m.version.as_str()), ("dan/demo1", "0.1.0"));
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "dan/demo1@0.1.0");
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain", "--ignored"]),
            ""
        );
        assert_eq!(fixture_git(&repo, &["branch", "--show-current"]), "main");
        assert_eq!(
            fixture_git(&repo, &["remote"]),
            "",
            "no remote is configured"
        );
        let files = fixture_git(&repo, &["ls-files"]);
        assert_eq!(files, ".gitignore\nREADME.md\ndist/index.js\nplugin.json");
        let readme = fs::read_to_string(repo.join("README.md")).unwrap();
        assert!(readme.contains("--ref dan/demo1@0.1.0"), "{readme}");
        assert!(readme.contains("--ref dan/demo1@0.1.1"), "{readme}");
        assert!(readme.contains("\ngitpm install "), "{readme}");
        assert!(readme.contains("gitpm --catalog ./catalog catalog-update"));
        assert!(!readme.contains("./pm"), "{readme}");
        assert!(readme.contains(&shell_quote(repo.to_str().unwrap())));
        assert_eq!(
            crate::releases::latest(&repo, None).unwrap().name,
            "dan/demo1@0.1.0"
        );
    }

    #[test]
    fn init_never_overwrites_and_validates_ids() {
        let dir = TempDir::new("scaffold");
        let existing = dir.join("demo1");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep.txt"), "mine").unwrap();
        let before = tree_hash(dir.path());
        let error = init(&existing, "dan", "demo1").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(
            error.to_string().contains("not a Git repository"),
            "{error}"
        );
        assert_eq!(tree_hash(dir.path()), before);

        for bad in ["..", "a b", "", "x/y", "-x"] {
            assert_eq!(
                init(&dir.join("new"), "dan", bad).unwrap_err().kind(),
                "invalid_id",
                "{bad:?}"
            );
            assert_eq!(
                init(&dir.join("new"), bad, "demo1").unwrap_err().kind(),
                "invalid_id",
                "{bad:?}"
            );
        }
        assert_eq!(validate_name("ok-name_1").map_err(|e| e.kind()), Ok(()));
        assert_eq!(
            tree_hash(dir.path()),
            before,
            "failed creates leave nothing behind"
        );
        // A single-package repository is not turned into a multi-package one.
        let single = new_repo(&dir, "single");
        let before = tree_hash(&single);
        assert_eq!(
            init(&single, "dan", "other").unwrap_err().kind(),
            "invalid_argument"
        );
        assert_eq!(tree_hash(&single), before);
    }

    #[test]
    fn suggests_multi_only_where_a_multi_package_repository_helps() {
        let dir = TempDir::new("scaffold");
        fs::create_dir(dir.join("empty")).unwrap();
        assert!(suggests_multi(&dir.join("empty")));
        let single = new_repo(&dir, "single");
        assert!(suggests_multi(&single));
        assert!(!suggests_multi(&dir.join("missing")));
        fs::create_dir(dir.join("full")).unwrap();
        fs::write(dir.join("full/x"), "x").unwrap();
        assert!(!suggests_multi(&dir.join("full")));
        let multi = dir.join("multi");
        init_with(
            &multi,
            "dan",
            "a",
            InitOptions {
                multi: true,
                release: true,
            },
        )
        .unwrap();
        assert!(!suggests_multi(&multi));
    }

    #[test]
    fn commit_package_returns_package_version_and_tag() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        fs::write(repo.join("dist/index.js"), "v2").unwrap();
        assert_eq!(
            commit_package(&repo, "0.2.0", Some("dan/demo1")).unwrap(),
            Commit {
                package: "dan/demo1".into(),
                version: "0.2.0".into(),
                tag: "dan/demo1@0.2.0".into(),
            }
        );
    }

    #[test]
    fn commit_bumps_version_commits_everything_and_tags() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        fs::write(repo.join("dist/index.js"), "v2").unwrap();
        fs::write(repo.join("dist/new.js"), "new").unwrap();
        let tag = commit(&repo, "0.2.0").unwrap().tag;
        assert_eq!(tag, "dan/demo1@0.2.0");
        assert_eq!(model::manifest(&repo).unwrap().version, "0.2.0");
        assert_eq!(fixture_git(&repo, &["status", "--porcelain"]), "");
        assert_eq!(
            fixture_git(&repo, &["rev-parse", "HEAD"]),
            fixture_git(&repo, &["rev-parse", &tag])
        );
        assert_eq!(
            fixture_git(&repo, &["log", "-1", "--format=%s"]),
            "Release dan/demo1 0.2.0"
        );
        let latest = crate::releases::latest(&repo, None).unwrap();
        assert_eq!(latest.manifest.version, "0.2.0");
        // The manifest stays in canonical form; the scaffold's title is kept.
        assert_eq!(
            fs::read_to_string(repo.join("plugin.json")).unwrap(),
            "{\n  \"id\": \"dan/demo1\",\n  \"version\": \"0.2.0\",\n  \"dependencies\": {},\n  \"title\": \"demo1\"\n}\n"
        );
        // A pre-release is released like any version and is a release tag, but not
        // the latest release.
        fs::write(repo.join("dist/index.js"), "v3 beta").unwrap();
        assert_eq!(
            commit(&repo, "0.3.0-beta.1").unwrap().tag,
            "dan/demo1@0.3.0-beta.1"
        );
        let found = crate::releases::discover(&repo, None).unwrap();
        assert_eq!(found.last().unwrap().manifest.version, "0.3.0-beta.1");
        assert_eq!(
            crate::releases::latest(&repo, None)
                .unwrap()
                .manifest
                .version,
            "0.2.0"
        );
    }

    #[test]
    fn commit_refusals_leave_the_repository_untouched() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        fs::write(repo.join("dist/index.js"), "pending edit").unwrap();
        // A tag for the next version already exists (for example created by hand).
        fixture_git(&repo, &["tag", "dan/demo1@0.3.0"]);
        let before = tree_hash(&repo);
        let cases = [
            ("0.1.0", "version_conflict"),
            ("0.3.0", "version_conflict"),
            ("0.2", "invalid_version"),
            ("1.0.0+build", "invalid_version"),
        ];
        for (version, kind) in cases {
            assert_eq!(
                commit(&repo, version).unwrap_err().kind(),
                kind,
                "{version}"
            );
        }
        assert_eq!(
            commit(&repo.join("dist"), "0.2.0").unwrap_err().kind(),
            "invalid_argument"
        );
        assert_eq!(
            commit(&dir.join("missing"), "0.2.0").unwrap_err().kind(),
            "io"
        );
        assert_eq!(tree_hash(&repo), before);
        assert_eq!(
            fs::read_to_string(repo.join("dist/index.js")).unwrap(),
            "pending edit"
        );
    }

    #[test]
    fn failed_commit_restores_manifest_and_index() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        fs::write(repo.join("dist/index.js"), "edit").unwrap();
        fs::write(repo.join("dist/staged.js"), "staged").unwrap();
        fixture_git(&repo, &["add", "dist/staged.js"]);
        let status = fixture_git(&repo, &["status", "--porcelain"]);
        let hook = repo.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let error = commit(&repo, "0.2.0").unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(error.to_string().contains("restored"), "{error}");
        assert_eq!(model::manifest(&repo).unwrap().version, "0.1.0");
        assert_eq!(fixture_git(&repo, &["status", "--porcelain"]), status);
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "dan/demo1@0.1.0");
    }

    #[test]
    fn commit_can_be_retried_after_a_failed_tag() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        fs::write(repo.join("dist/index.js"), "v2").unwrap();
        // A stale ref lock makes tag creation fail after the commit.
        let lock = repo.join(".git/refs/tags/dan/demo1@0.2.0.lock");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        fs::write(&lock, "").unwrap();
        let error = commit(&repo, "0.2.0").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("releasing the same version again"),
            "{error}"
        );
        let head = fixture_git(&repo, &["rev-parse", "HEAD"]);
        fs::remove_file(&lock).unwrap();
        assert_eq!(commit(&repo, "0.2.0").unwrap().tag, "dan/demo1@0.2.0");
        assert_eq!(fixture_git(&repo, &["rev-parse", "dan/demo1@0.2.0"]), head);
        // Once tagged, the same version is a conflict again.
        assert_eq!(
            commit(&repo, "0.2.0").unwrap_err().kind(),
            "version_conflict"
        );
        // An edited tree after the release commit is not a retry.
        fs::write(repo.join("dist/index.js"), "v3").unwrap();
        fixture_git(&repo, &["tag", "-d", "dan/demo1@0.2.0"]);
        assert_eq!(
            commit(&repo, "0.2.0").unwrap_err().kind(),
            "version_conflict"
        );
    }

    #[test]
    fn set_remote_adds_or_replaces_origin_without_network() {
        let dir = TempDir::new("scaffold");
        let repo = new_repo(&dir, "demo1");
        set_remote(&repo, "git@example.com:dan/demo1.git").unwrap();
        assert_eq!(
            fixture_git(&repo, &["remote", "get-url", "origin"]),
            "git@example.com:dan/demo1.git"
        );
        fixture_git(
            &repo,
            &[
                "config",
                "--add",
                "remote.origin.url",
                "https://old.example/second.git",
            ],
        );
        fixture_git(
            &repo,
            &[
                "config",
                "remote.origin.pushurl",
                "https://old.example/push.git",
            ],
        );
        set_remote(&repo, "https://example.com/dan/demo1.git").unwrap();
        assert_eq!(
            fixture_git(&repo, &["config", "--get-all", "remote.origin.url"]),
            "https://example.com/dan/demo1.git"
        );
        assert_eq!(
            fixture_git(&repo, &["config", "--get-all", "remote.origin.pushurl"]),
            "https://example.com/dan/demo1.git"
        );

        let before = tree_hash(&repo);
        for bad in ["", "  ", "-uevil", "https://x\n.git"] {
            assert_eq!(
                set_remote(&repo, bad).unwrap_err().kind(),
                "invalid_argument",
                "{bad:?}"
            );
        }
        assert_eq!(
            set_remote(&repo.join("dist"), "https://x")
                .unwrap_err()
                .kind(),
            "invalid_argument"
        );
        assert_eq!(tree_hash(&repo), before);
        fs::remove_file(repo.join("plugin.json")).unwrap();
        assert_eq!(
            set_remote(&repo, "https://x").unwrap_err().kind(),
            "invalid_manifest"
        );
    }

    #[test]
    fn packages_are_added_to_a_multi_package_repository_and_released_by_id() {
        let dir = TempDir::new("scaffold");
        let repo = dir.join("mono");
        // A missing folder becomes a new repository with gitpm.json.
        let store = create_package(&repo, "tilr", "store").unwrap();
        assert!(store.ends_with("packages/store"));
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "tilr/store@0.1.0");
        let theme = create_package(&repo, "tilr", "theme").unwrap();
        assert!(theme.join("dist/index.js").is_file());
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain"]),
            "",
            "everything committed"
        );
        assert_eq!(
            create_package(&repo, "tilr", "theme").unwrap_err().kind(),
            "invalid_argument"
        );
        // Unrelated edits are not committed with a new package.
        fs::write(repo.join("notes.txt"), "draft").unwrap();
        create_package(&repo, "tilr", "third").unwrap();
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain"]),
            "?? notes.txt"
        );
        fs::remove_file(repo.join("notes.txt")).unwrap();

        // commit needs the package when several exist; the tag is ID@VERSION.
        fs::write(theme.join("dist/index.js"), "theme v2").unwrap();
        let error = commit(&repo, "0.2.0").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(error.to_string().contains("tilr/store"), "{error}");
        assert_eq!(
            commit_package(&repo, "0.2.0", Some("tilr/theme")).unwrap(),
            Commit {
                package: "tilr/theme".into(),
                version: "0.2.0".into(),
                tag: "tilr/theme@0.2.0".into(),
            }
        );
        let m = model::package_manifest(&repo, Some("packages/theme")).unwrap();
        assert_eq!(m.version, "0.2.0");
        let latest = crate::releases::latest(&repo, Some("tilr/theme")).unwrap();
        assert_eq!(latest.path, "packages/theme");

        // release_current tags the version the manifest has.
        assert_eq!(
            release_current(&repo, Some("tilr/third"))
                .unwrap_err()
                .kind(),
            "version_conflict"
        );

        // A root package is not converted implicitly.
        let single = dir.join("single");
        init(&single, "tilr", "single").unwrap();
        assert_eq!(
            create_package(&single, "tilr", "extra").unwrap_err().kind(),
            "invalid_argument"
        );
        assert!(!single.join(packages::CONFIG).exists());
    }

    /// Everything of a repository except its object store (a failed `git add` leaves
    /// unreachable loose objects there).
    fn snapshot(repo: &Path) -> String {
        crate::test_support::tree_hash_except(repo, &[".git/objects"])
    }

    #[test]
    fn create_package_refuses_uncommitted_config_and_a_missing_identity() {
        if isolated_identity_test(
            "scaffold::tests::create_package_refuses_uncommitted_config_and_a_missing_identity",
        ) {
            return;
        }
        let dir = TempDir::new("scaffold");
        let repo = dir.join("mono");
        create_package(&repo, "tilr", "store").unwrap();
        fs::write(
            repo.join(packages::CONFIG),
            "{\n  \"packages\": [\"packages/*\", \"extra\"]\n}\n",
        )
        .unwrap();
        let before = snapshot(&repo);
        let error = create_package(&repo, "tilr", "theme").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(
            error
                .to_string()
                .starts_with("commit or discard the changes to gitpm.json"),
            "{error}"
        );
        assert_eq!(snapshot(&repo), before);
        fixture_git(&repo, &["checkout", "-q", "--", packages::CONFIG]);
        // gitpm never writes an identity into an existing repository.
        fixture_git(&repo, &["config", "--unset", "user.name"]);
        fixture_git(&repo, &["config", "--unset", "user.email"]);
        let before = snapshot(&repo);
        let error = create_package(&repo, "tilr", "theme").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert_eq!(
            error.to_string(),
            "no Git identity configured; set user.name and user.email"
        );
        assert_eq!(snapshot(&repo), before);
        assert!(
            !fs::read_to_string(repo.join(".git/config"))
                .unwrap()
                .contains("name =")
        );
        // An existing folder is refused like in a fork that adds packages.
        fs::create_dir_all(repo.join("packages/theme")).unwrap();
        fs::write(repo.join("packages/theme/notes.txt"), "mine").unwrap();
        let error = create_package(&repo, "tilr", "theme").unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(error.to_string().contains("already exists"), "{error}");
    }

    #[test]
    fn create_package_keeps_the_commit_when_only_the_tag_fails() {
        let dir = TempDir::new("scaffold");
        let repo = dir.join("mono");
        create_package(&repo, "tilr", "store").unwrap();
        fs::write(repo.join("notes.txt"), "draft").unwrap();
        let before = fixture_git(&repo, &["rev-parse", "HEAD"]);
        // A stale ref lock makes tag creation fail after the commit.
        let lock = repo.join(".git/refs/tags/tilr/theme@0.1.0.lock");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        fs::write(&lock, "").unwrap();
        let error = create_package(&repo, "tilr", "theme").unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(
            error.to_string().contains("was committed")
                && error.to_string().contains("git tag tilr/theme@0.1.0"),
            "{error}"
        );
        // The working tree is untouched: clean against the new commit, edits kept.
        assert_eq!(
            fixture_git(&repo, &["rev-parse", "HEAD~1"]),
            before,
            "one new commit"
        );
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain"]),
            "?? notes.txt"
        );
        assert!(repo.join("packages/theme/plugin.json").is_file());
        fs::remove_file(&lock).unwrap();
        fixture_git(&repo, &["tag", "tilr/theme@0.1.0"]);
        assert_eq!(
            crate::releases::latest(&repo, Some("tilr/theme"))
                .unwrap()
                .name,
            "tilr/theme@0.1.0"
        );
    }

    #[test]
    fn create_package_restores_the_repository_when_the_commit_fails() {
        let dir = TempDir::new("scaffold");
        let repo = dir.join("mono");
        create_package(&repo, "tilr", "store").unwrap();
        let hook = repo.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let before = snapshot(&repo);
        assert_eq!(
            create_package(&repo, "tilr", "theme").unwrap_err().kind(),
            "git"
        );
        assert_eq!(snapshot(&repo), before);
    }

    #[test]
    fn new_packages_follow_the_first_folder_pattern() {
        let config = |patterns: &[&str]| packages::Config {
            packages: patterns.iter().map(|p| (*p).to_owned()).collect(),
        };
        assert_eq!(package_folder(&config(&["apps/*"]), "x"), "apps/x");
        assert_eq!(
            package_folder(&config(&["special", "packages/*"]), "x"),
            "packages/x"
        );
        assert_eq!(package_folder(&config(&["a*/*"]), "x"), "packages/x");
        assert_eq!(package_folder(&config(&[]), "x"), "packages/x");
    }
}
