//! Multi-package repositories. A root `gitpm.json` (`{"packages": ["packages/*"]}`) lists
//! package folders by glob; each folder holds its own `plugin.json` and `dist/`. Without
//! `gitpm.json` the repository root is the only package (the original layout).
//!
//! Everything here reads committed trees (`git ls-tree`/`git show` at a commit), never a
//! working tree: the package set of a release is the one in its tagged commit.
use crate::error::{Error, Result, ensure};
use crate::{Manifest, transport::git, transport::git_raw, valid_id};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// Name of the repository-level config file.
pub const CONFIG: &str = "gitpm.json";

/// `gitpm.json`: package folders as glob patterns relative to the repository root.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub packages: Vec<String>,
}

/// One package of a repository at a commit.
#[derive(Clone, Debug, Serialize)]
pub struct Package {
    /// Folder relative to the repository root, `/`-separated; empty for the root package.
    pub path: String,
    pub manifest: Manifest,
}
impl Package {
    /// `None` for the root package (as stored in state and catalog).
    pub fn subdir(&self) -> Option<&str> {
        (!self.path.is_empty()).then_some(self.path.as_str())
    }
}

/// A pattern is a relative path of `/`-separated segments; `*` in a segment matches any
/// run of characters within that segment (no `**`, no `..`, no hidden folders: a segment
/// cannot start with `.`).
pub fn validate_pattern(pattern: &str) -> Result<()> {
    ensure!(
        !pattern.is_empty()
            && !pattern.starts_with('/')
            && !pattern.contains('\\')
            && pattern
                .split('/')
                .all(|s| !s.is_empty() && !s.starts_with('.') && s != "**"),
        InvalidManifest,
        "invalid package pattern in {CONFIG}: {pattern:?} (use relative folders such as packages/*; no '..', '**' or hidden folders)"
    );
    Ok(())
}

fn segment_matches(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if name.len() < first.len() + last.len() || !name.starts_with(first) || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(index) => rest = &rest[index + middle.len()..],
            None => return false,
        }
    }
    true
}

/// Whether a folder path matches a package pattern (segment by segment).
pub fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, path): (Vec<_>, Vec<_>) =
        (pattern.split('/').collect(), path.split('/').collect());
    pattern.len() == path.len()
        && pattern
            .iter()
            .zip(&path)
            .all(|(pattern, name)| segment_matches(pattern, name))
}

/// A stored package folder: relative, normal components only.
pub(crate) fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".." && !s.contains('\0')),
        InvalidData,
        "invalid package path {path:?}"
    );
    Ok(())
}

/// Mode and type of every entry of the commit's tree (recursive, folders included).
fn tree(repo: &Path, commit: &str, custom_ssh: bool) -> Result<BTreeMap<String, (String, String)>> {
    let raw = git_raw(
        repo,
        &["ls-tree", "-r", "-t", "-z", "--full-tree", commit, "--"],
        custom_ssh,
    )?;
    let mut entries = BTreeMap::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut meta = meta.split(' ');
        let (Some(mode), Some(kind)) = (meta.next(), meta.next()) else {
            continue;
        };
        entries.insert(path.to_owned(), (mode.to_owned(), kind.to_owned()));
    }
    Ok(entries)
}

fn show(repo: &Path, commit: &str, path: &str, custom_ssh: bool) -> Result<String> {
    git(repo, &["show", &format!("{commit}:{path}")], custom_ssh)
}

/// Read and validate the manifest committed at a package's repository-relative path.
pub(crate) fn manifest_at(
    repo: &Path,
    commit: &str,
    package: Option<&str>,
    custom_ssh: bool,
) -> Result<Manifest> {
    let path = match package.filter(|p| !p.is_empty()) {
        Some(folder) => format!("{folder}/plugin.json"),
        None => "plugin.json".to_owned(),
    };
    let manifest: Manifest = serde_json::from_str(&show(repo, commit, &path, custom_ssh)?)
        .map_err(|e| Error::invalid_manifest(format!("invalid {path}: {e}")))?;
    manifest.validate_dist().map_err(|mut error| {
        if let Error::InvalidManifest { id, .. } = &mut error {
            *id = Some(manifest.id.clone());
        }
        error
    })?;
    Ok(manifest)
}

/// `gitpm.json` at `commit`; `None` for a single-package repository.
pub fn config_at(repo: &Path, commit: &str) -> Result<Option<Config>> {
    config_at_with_ssh(repo, commit, false)
}

pub(crate) fn config_at_with_ssh(
    repo: &Path,
    commit: &str,
    custom_ssh: bool,
) -> Result<Option<Config>> {
    let listed = git(
        repo,
        &["ls-tree", "--name-only", commit, "--", CONFIG],
        custom_ssh,
    )?;
    if listed.is_empty() {
        return Ok(None);
    }
    let config: Config = serde_json::from_str(&show(repo, commit, CONFIG, custom_ssh)?)
        .map_err(|e| Error::invalid_manifest(format!("invalid {CONFIG}: {e}")))?;
    for pattern in &config.packages {
        validate_pattern(pattern)?;
    }
    Ok(Some(config))
}

/// A folder that `gitpm.json` lists (it holds a `plugin.json`) but that is not a usable
/// package at that commit. `id` is set when its manifest could be read.
#[derive(Clone, Debug)]
pub struct Problem {
    pub path: String,
    pub id: Option<String>,
    pub error: Error,
}

/// Package set of a commit: the valid packages and the listed folders that are not. A
/// defect of one folder does not affect the others; only selecting a package checks its
/// own folder strictly (see [`Layout::select`]).
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Sorted by folder.
    pub packages: Vec<Package>,
    /// Sorted by folder.
    pub problems: Vec<Problem>,
}
impl Layout {
    /// The package `id` (the only package when `id` is `None`). A defect in its own
    /// folder, or another folder claiming the same ID (in any letter case), is an error;
    /// defects of other folders are not. `at` names the commit in messages.
    pub fn select(self, id: Option<&str>, at: &str) -> Result<Package> {
        let Some(id) = id else {
            if self.problems.is_empty() {
                return choose(self.packages, None, at);
            }
            if let ([], [problem]) = (&self.packages[..], &self.problems[..]) {
                return Err(problem.error.clone());
            }
            return Err(Error::InvalidArgument(format!(
                "repository has {} packages{at} and folders that are not packages; choose one by ID: {}{}",
                self.packages.len(),
                list(&self.packages),
                self.problem_list()
            )));
        };
        if let Some(problem) = self.problems.iter().find(|p| {
            p.id.as_deref()
                .is_some_and(|other| other.eq_ignore_ascii_case(id))
        }) {
            return Err(problem.error.clone());
        }
        let problems = self.problem_list();
        choose(self.packages, Some(id), at).map_err(|error| match error {
            Error::InvalidArgument(message) => {
                Error::InvalidArgument(format!("{message}{problems}"))
            }
            other => other,
        })
    }
    fn problem_list(&self) -> String {
        if self.problems.is_empty() {
            return String::new();
        }
        let items: Vec<String> = self
            .problems
            .iter()
            .map(|p| format!("{} ({})", p.path, p.error))
            .collect();
        format!("; not packages: {}", items.join("; "))
    }
    /// Every listed folder, valid or not (sorted).
    pub fn folders(&self) -> Vec<&str> {
        let mut folders: Vec<&str> = self
            .packages
            .iter()
            .map(|p| p.path.as_str())
            .chain(self.problems.iter().map(|p| p.path.as_str()))
            .collect();
        folders.sort_unstable();
        folders
    }
}

/// One listed folder at a commit: its package, or why it is none (with the ID if known).
fn read_folder(
    repo: &Path,
    commit: &str,
    entries: &BTreeMap<String, (String, String)>,
    folder: &str,
    custom_ssh: bool,
) -> std::result::Result<Package, (Option<String>, Error)> {
    let path = format!("{folder}/plugin.json");
    let (mode, kind) = &entries[&path];
    if kind != "blob" || mode == "120000" {
        return Err((
            None,
            Error::Unsupported(format!("symlinks are not supported: {path}")),
        ));
    }
    let manifest = manifest_at(repo, commit, Some(folder), custom_ssh).map_err(|error| {
        let id = match &error {
            Error::InvalidManifest { id, .. } => id.clone(),
            _ => None,
        };
        (id, error)
    })?;
    let id = Some(manifest.id.clone());
    valid_id(&manifest.id).map_err(|e| (id.clone(), e))?;
    let output = manifest.dist_path(Some(folder));
    match entries.get(&output) {
        Some((_, kind)) if kind == "tree" => {}
        Some(_) => {
            return Err((
                id,
                Error::Unsupported(format!("symlinks are not supported: {output}")),
            ));
        }
        None => {
            return Err((
                id,
                manifest.missing_dist(format!(
                    "package {folder} must contain a ready {}/ directory",
                    manifest.dist_folder()
                )),
            ));
        }
    }
    Ok(Package {
        path: folder.to_owned(),
        manifest,
    })
}

/// Packages and defective folders listed by `gitpm.json` at `commit`; `None` without
/// `gitpm.json`. A package needs a `plugin.json` file (not a symlink), a committed `dist/`
/// folder and a `namespace/name` ID that no other folder uses (in any letter case);
/// nested folders are defective. Only an invalid `gitpm.json` itself is an error.
pub fn layout_at(repo: &Path, commit: &str) -> Result<Option<Layout>> {
    layout_at_with_ssh(repo, commit, false)
}

pub(crate) fn layout_at_with_ssh(
    repo: &Path,
    commit: &str,
    custom_ssh: bool,
) -> Result<Option<Layout>> {
    let Some(config) = config_at_with_ssh(repo, commit, custom_ssh)? else {
        return Ok(None);
    };
    let entries = tree(repo, commit, custom_ssh)?;
    let folders: Vec<&str> = entries
        .keys()
        .filter_map(|path| path.strip_suffix("/plugin.json"))
        .filter(|folder| config.packages.iter().any(|p| matches(p, folder)))
        .collect();
    let mut layout = Layout::default();
    for folder in &folders {
        match read_folder(repo, commit, &entries, folder, custom_ssh) {
            Ok(package) => layout.packages.push(package),
            Err((id, error)) => layout.problems.push(Problem {
                path: (*folder).to_owned(),
                id,
                error,
            }),
        }
    }
    // Ambiguous IDs and nested folders make every folder involved defective.
    let mut broken: BTreeMap<String, (Option<String>, Error)> = BTreeMap::new();
    for a in &layout.packages {
        for b in &layout.packages {
            if a.path != b.path && a.manifest.id.eq_ignore_ascii_case(&b.manifest.id) {
                let (first, second) = if a.path < b.path { (a, b) } else { (b, a) };
                broken.insert(
                    a.path.clone(),
                    (
                        Some(a.manifest.id.clone()),
                        Error::invalid_manifest(format!(
                            "package ID {} appears in both {} and {}",
                            a.manifest.id, first.path, second.path
                        )),
                    ),
                );
            }
        }
        for other in &folders {
            let (inner, outer) = if other.starts_with(&format!("{}/", a.path)) {
                (*other, a.path.as_str())
            } else if a.path.starts_with(&format!("{other}/")) {
                (a.path.as_str(), *other)
            } else {
                continue;
            };
            broken.insert(
                a.path.clone(),
                (
                    Some(a.manifest.id.clone()),
                    Error::invalid_manifest(format!(
                        "package {inner} is nested inside package {outer}"
                    )),
                ),
            );
        }
    }
    let packages = std::mem::take(&mut layout.packages);
    for package in packages {
        match broken.remove(&package.path) {
            Some((id, error)) => layout.problems.push(Problem {
                path: package.path,
                id,
                error,
            }),
            None => layout.packages.push(package),
        }
    }
    layout.problems.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(layout))
}

/// Packages listed by `gitpm.json` at `commit`, sorted by folder; `None` without
/// `gitpm.json`. Strict: any defective folder (see [`layout_at`]) is an error.
pub fn packages_at(repo: &Path, commit: &str) -> Result<Option<Vec<Package>>> {
    packages_at_with_ssh(repo, commit, false)
}

pub(crate) fn packages_at_with_ssh(
    repo: &Path,
    commit: &str,
    custom_ssh: bool,
) -> Result<Option<Vec<Package>>> {
    let Some(layout) = layout_at_with_ssh(repo, commit, custom_ssh)? else {
        return Ok(None);
    };
    if let Some(problem) = layout.problems.into_iter().next() {
        return Err(problem.error);
    }
    Ok(Some(layout.packages))
}

fn list(packages: &[Package]) -> String {
    if packages.is_empty() {
        return "none".into();
    }
    packages
        .iter()
        .map(|p| format!("{} ({})", p.manifest.id, p.path))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The package `id` at `commit` (any package for a single-package repository, the only
/// one of a one-package `gitpm.json`). Without `id` a repository with several packages
/// is refused with the list of its packages. Only the selected package's folder must be
/// valid (see [`Layout::select`]).
pub fn find_at(repo: &Path, commit: &str, id: Option<&str>) -> Result<Package> {
    find_at_with_ssh(repo, commit, id, false)
}

pub(crate) fn find_at_with_ssh(
    repo: &Path,
    commit: &str,
    id: Option<&str>,
    custom_ssh: bool,
) -> Result<Package> {
    if let Some(id) = id {
        valid_id(id)?;
    }
    let Some(layout) = layout_at_with_ssh(repo, commit, custom_ssh)? else {
        ensure!(
            !git(
                repo,
                &["ls-tree", "--name-only", commit, "--", "plugin.json"],
                custom_ssh
            )?
            .is_empty(),
            InvalidManifest,
            "missing plugin.json in the repository root (and no {CONFIG} listing packages)"
        );
        let manifest = manifest_at(repo, commit, None, custom_ssh)?;
        if let Some(id) = id {
            ensure!(
                manifest.id == id,
                InvalidArgument,
                "repository contains package {}, not {id}",
                manifest.id
            );
        }
        return Ok(Package {
            path: String::new(),
            manifest,
        });
    };
    let at = format!(" at commit {}", &commit[..commit.len().min(12)]);
    layout.select(id, &at)
}

/// Pick `id` from a package list (the only package when `id` is `None`).
pub(crate) fn choose(packages: Vec<Package>, id: Option<&str>, at: &str) -> Result<Package> {
    match id {
        Some(id) => match packages.iter().position(|p| p.manifest.id == id) {
            Some(index) => Ok(packages[index].clone()),
            None => Err(Error::InvalidArgument(format!(
                "package {id} is not in this repository{at}; packages there: {}",
                list(&packages)
            ))),
        },
        None if packages.len() == 1 => Ok(packages[0].clone()),
        None => Err(Error::InvalidArgument(format!(
            "repository has {} packages{at}; choose one by ID: {}",
            packages.len(),
            list(&packages)
        ))),
    }
}

/// `gitpm.json` of a working tree; `None` if the file does not exist.
pub fn read_config(repo: &Path) -> Result<Option<Config>> {
    let path = repo.join(CONFIG);
    if fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    let config: Config = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|e| Error::invalid_manifest(format!("invalid {CONFIG}: {e}")))?;
    for pattern in &config.packages {
        validate_pattern(pattern)?;
    }
    Ok(Some(config))
}

/// Packages of a working tree (author side, before anything is committed): folders that
/// match a `gitpm.json` pattern and hold a `plugin.json`, sorted; symlinked folders are
/// skipped. `None` without `gitpm.json`.
pub fn packages_in(repo: &Path) -> Result<Option<Vec<Package>>> {
    let Some(config) = read_config(repo)? else {
        return Ok(None);
    };
    let mut folders = std::collections::BTreeSet::new();
    for pattern in &config.packages {
        let mut level = vec![String::new()];
        for segment in pattern.split('/') {
            let mut next = Vec::new();
            for parent in &level {
                let dir = if parent.is_empty() {
                    repo.to_path_buf()
                } else {
                    repo.join(parent)
                };
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries {
                    let entry = entry?;
                    let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    if name == ".git" || !entry.file_type()?.is_dir() {
                        continue;
                    }
                    if segment_matches(segment, &name) {
                        next.push(if parent.is_empty() {
                            name
                        } else {
                            format!("{parent}/{name}")
                        });
                    }
                }
            }
            level = next;
        }
        folders.extend(
            level
                .into_iter()
                .filter(|folder| repo.join(folder).join("plugin.json").is_file()),
        );
    }
    let mut packages = Vec::new();
    for path in folders {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(repo.join(&path).join("plugin.json"))?)
                .map_err(|e| Error::invalid_manifest(format!("invalid {path}/plugin.json: {e}")))?;
        manifest.validate_dist()?;
        packages.push(Package { path, manifest });
    }
    Ok(Some(packages))
}

/// The package ID a release tag names (`namespace/name@VERSION`), if it looks like one.
pub(crate) fn tag_id(tag: &str) -> Option<&str> {
    let (id, version) = tag.rsplit_once('@')?;
    (valid_id(id).is_ok() && crate::catalog::version_key(version).is_ok()).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempDir, git as fixture_git};

    #[test]
    fn patterns_match_segment_by_segment() {
        assert!(matches("packages/*", "packages/store"));
        assert!(!matches("packages/*", "packages/store/sub"));
        assert!(!matches("packages/*", "other/store"));
        assert!(!matches("packages/*", "packages/.hidden"));
        assert!(matches("apps/theme", "apps/theme"));
        assert!(matches("packages/gitpm-*", "packages/gitpm-store"));
        assert!(!matches("packages/gitpm-*", "packages/store"));
        assert!(matches("a*b*c", "aXbYc"));
        assert!(!matches("a*b*c", "aXc"));
        assert!(matches("*/*", "x/y"));
        // No hidden folders: neither literal nor leading-dot segments.
        for bad in [
            "",
            "/abs",
            "a/../b",
            "./a",
            "a//b",
            "**/x",
            "a\\b",
            ".x",
            ".*",
            "packages/.cache",
            "a/.*",
            "..",
        ] {
            assert_eq!(
                validate_pattern(bad).unwrap_err().kind(),
                "invalid_manifest",
                "{bad:?}"
            );
        }
        for bad in ["", "/a", "a/..", "a//b", "."] {
            assert!(validate_path(bad).is_err(), "{bad:?}");
        }
        assert!(validate_path("packages/store").is_ok());
    }

    #[test]
    fn tag_ids_need_a_namespace_and_a_version() {
        assert_eq!(tag_id("tilr/store@1.2.3"), Some("tilr/store"));
        assert_eq!(tag_id("v1.2.3"), None);
        assert_eq!(tag_id("store@1.2.3"), None);
        assert_eq!(tag_id("tilr/store@banana"), None);
    }

    fn package(repo: &Path, folder: &str, id: &str) {
        fs::create_dir_all(repo.join(folder).join("dist")).unwrap();
        fs::write(
            repo.join(folder).join("plugin.json"),
            format!(r#"{{"id":"{id}","version":"1.0.0"}}"#),
        )
        .unwrap();
        fs::write(repo.join(folder).join("dist/index.js"), id).unwrap();
    }
    fn commit(repo: &Path) -> String {
        fixture_git(repo, &["add", "--all"]);
        fixture_git(repo, &["commit", "-q", "-m", "change"]);
        fixture_git(repo, &["rev-parse", "HEAD"])
    }

    #[test]
    fn invalid_output_path_keeps_package_identity() {
        let dir = TempDir::new("invalid-output-path");
        let repo = dir.join("mono");
        fs::create_dir_all(&repo).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(CONFIG), r#"{"packages":["packages/*"]}"#).unwrap();
        package(&repo, "packages/good", "tilr/good");
        package(&repo, "packages/bad", "tilr/bad");
        fs::write(
            repo.join("packages/bad/plugin.json"),
            r#"{"id":"tilr/bad","version":"1.0.0","dist":"../escape"}"#,
        )
        .unwrap();
        let head = commit(&repo);
        let layout = layout_at(&repo, &head).unwrap().unwrap();
        assert_eq!(layout.problems.len(), 1);
        assert_eq!(layout.problems[0].id.as_deref(), Some("tilr/bad"));
        assert!(find_at(&repo, &head, Some("tilr/good")).is_ok());
        let error = find_at(&repo, &head, Some("tilr/bad")).unwrap_err();
        assert_eq!(error.kind(), "invalid_manifest");
        assert!(error.to_string().contains("invalid manifest dist path"));
    }

    #[test]
    fn packages_are_read_from_the_commit_tree() {
        let dir = TempDir::new("packages");
        let repo = dir.join("mono");
        fs::create_dir_all(&repo).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(CONFIG), r#"{"packages":["packages/*"]}"#).unwrap();
        package(&repo, "packages/store", "tilr/store");
        package(&repo, "packages/theme", "tilr/theme");
        package(&repo, "other/skip", "tilr/skip");
        let first = commit(&repo);
        let found = packages_at(&repo, &first).unwrap().unwrap();
        assert_eq!(
            found.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
            ["packages/store", "packages/theme"]
        );
        assert_eq!(
            find_at(&repo, &first, Some("tilr/theme")).unwrap().path,
            "packages/theme"
        );
        let error = find_at(&repo, &first, None).unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(
            error.to_string().contains("tilr/store (packages/store)")
                && error.to_string().contains("tilr/theme (packages/theme)"),
            "{error}"
        );
        let error = find_at(&repo, &first, Some("tilr/skip")).unwrap_err();
        assert!(
            error.to_string().contains("not in this repository"),
            "{error}"
        );

        // The working tree does not matter, only the commit.
        fs::remove_dir_all(repo.join("packages/theme")).unwrap();
        assert!(find_at(&repo, &first, Some("tilr/theme")).is_ok());

        // Without gitpm.json the root is the package.
        let single = dir.join("single");
        fs::create_dir_all(&single).unwrap();
        fixture_git(&single, &["init", "-q", "-b", "main"]);
        package(&single, ".", "tilr/single");
        let head = commit(&single);
        assert!(packages_at(&single, &head).unwrap().is_none());
        let root = find_at(&single, &head, None).unwrap();
        assert_eq!((root.path.as_str(), root.subdir()), ("", None));
        assert_eq!(
            find_at(&single, &head, Some("tilr/other"))
                .unwrap_err()
                .kind(),
            "invalid_argument"
        );
    }

    #[test]
    fn ids_differing_only_in_case_make_both_folders_defective() {
        let dir = TempDir::new("packages");
        let repo = dir.join("mono");
        fs::create_dir_all(&repo).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(CONFIG), r#"{"packages":["packages/*"]}"#).unwrap();
        package(&repo, "packages/a", "tilr/Same");
        package(&repo, "packages/b", "tilr/same");
        package(&repo, "packages/c", "tilr/other");
        let head = commit(&repo);
        let layout = layout_at(&repo, &head).unwrap().unwrap();
        assert_eq!(
            layout
                .packages
                .iter()
                .map(|p| p.path.as_str())
                .collect::<Vec<_>>(),
            ["packages/c"]
        );
        let problems: Vec<(&str, Option<&str>)> = layout
            .problems
            .iter()
            .map(|p| (p.path.as_str(), p.id.as_deref()))
            .collect();
        assert_eq!(
            problems,
            [
                ("packages/a", Some("tilr/Same")),
                ("packages/b", Some("tilr/same"))
            ]
        );
        for problem in &layout.problems {
            assert_eq!(problem.error.kind(), "invalid_manifest");
            assert!(
                problem
                    .error
                    .to_string()
                    .contains("appears in both packages/a and packages/b"),
                "{}",
                problem.error
            );
        }
        // Selecting either spelling (or a third one) names the conflict.
        for id in ["tilr/same", "tilr/Same", "TILR/SAME"] {
            let error = layout.clone().select(Some(id), "").unwrap_err();
            assert_eq!(error.kind(), "invalid_manifest", "{id}");
            assert!(error.to_string().contains("appears in both"), "{error}");
        }
        // The sibling is unaffected; the strict listing refuses.
        assert_eq!(
            layout.clone().select(Some("tilr/other"), "").unwrap().path,
            "packages/c"
        );
        assert_eq!(
            find_at(&repo, &head, Some("tilr/same")).unwrap_err().kind(),
            "invalid_manifest"
        );
        assert!(packages_at(&repo, &head).is_err());
    }

    #[test]
    fn invalid_package_sets_are_refused() {
        let dir = TempDir::new("packages");
        let repo = dir.join("mono");
        fs::create_dir_all(&repo).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(CONFIG), r#"{"packages":["packages/*"]}"#).unwrap();
        package(&repo, "packages/a", "tilr/same");
        package(&repo, "packages/b", "tilr/same");
        let head = commit(&repo);
        let error = packages_at(&repo, &head).unwrap_err();
        assert!(error.to_string().contains("appears in both"), "{error}");

        package(&repo, "packages/b", "legacy");
        let head = commit(&repo);
        assert_eq!(packages_at(&repo, &head).unwrap_err().kind(), "invalid_id");

        package(&repo, "packages/b", "tilr/b");
        fs::remove_dir_all(repo.join("packages/b/dist")).unwrap();
        std::os::unix::fs::symlink("../a/dist", repo.join("packages/b/dist")).unwrap();
        let head = commit(&repo);
        assert_eq!(packages_at(&repo, &head).unwrap_err().kind(), "unsupported");

        fs::remove_file(repo.join("packages/b/dist")).unwrap();
        fs::write(repo.join("packages/b/README"), "no dist").unwrap();
        let head = commit(&repo);
        let error = packages_at(&repo, &head).unwrap_err();
        assert!(error.to_string().contains("ready dist/"), "{error}");

        fs::write(repo.join(CONFIG), r#"{"packages":["../x"]}"#).unwrap();
        let head = commit(&repo);
        assert_eq!(
            packages_at(&repo, &head).unwrap_err().kind(),
            "invalid_manifest"
        );
        fs::write(repo.join(CONFIG), r#"{"packages":[],"extra":1}"#).unwrap();
        let head = commit(&repo);
        assert_eq!(
            packages_at(&repo, &head).unwrap_err().kind(),
            "invalid_manifest"
        );
    }
}
