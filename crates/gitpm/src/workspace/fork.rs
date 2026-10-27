//! Fork installed packages or an explicit source without a workspace.
use super::*;
use std::collections::BTreeSet;

/// One package forked by [`Manager::fork_packages`].
#[derive(Clone, Debug, Serialize)]
pub struct ForkedPackage {
    pub original: String,
    pub id: String,
    /// Package folder inside the fork repository; empty for a root package (the fork
    /// repository is then that package, without `gitpm.json`).
    pub path: String,
    /// Version in the fork's `plugin.json` (the forked version).
    pub version: String,
}
/// Result of [`Manager::fork_packages`].
#[derive(Clone, Debug, Serialize)]
pub struct PackageFork {
    /// The fork repository (author folder, outside the workspace).
    pub path: PathBuf,
    /// Always `true`: forks are created in new repositories.
    pub created: bool,
    /// The commit that created the fork identities.
    pub commit: String,
    pub packages: Vec<ForkedPackage>,
}
/// One package to fork: where it comes from and what it becomes.
struct Source {
    original: String,
    /// Recorded source of the original (`packages.<original>.source`).
    original_source: String,
    id: String,
    /// Installed revision to read from the recorded source.
    commit: String,
    /// Folder at that commit; empty for a root package.
    path: String,
    /// Manifest at that commit.
    base: Manifest,
}
impl Source {
    fn subdir(&self) -> Option<&str> {
        (!self.path.is_empty()).then_some(self.path.as_str())
    }
}

fn manifest_path(folder: &str) -> String {
    if folder.is_empty() {
        "plugin.json".to_owned()
    } else {
        format!("{folder}/plugin.json")
    }
}

impl Manager {
    /// Fork one installed package with a fresh UUID: [`Manager::fork_packages`]
    /// with one ID. Returns the fork repository.
    pub fn fork_into(&self, id: &str, destination: &Path) -> Result<PathBuf> {
        Ok(self.fork_packages(&[id], destination)?.path)
    }
    /// Fork installed packages of one repository with fresh UUIDs into `destination`.
    ///
    /// The destination must be a new folder outside the workspace. Clone the recorded
    /// source with history on a new `fork` branch, retaining submodule Git links.
    /// Installed packages start at their recorded revisions; local installation work
    /// is ignored. Other packages start at the newest installed revision. All packages
    /// in the repository receive fresh identities, including uninstalled siblings;
    /// names, repository files and the package layout are preserved.
    /// Each manifest gets its new ID, `forkOf` and `replaces`; dependencies among
    /// packages forked together are preserved until explicitly edited. Nothing is tagged.
    pub fn fork_packages(&self, ids: &[&str], destination: &Path) -> Result<PackageFork> {
        self.fork_packages_with(ids, destination)
    }

    /// Absolute fork repository `destination`: refused with `..`, inside the workspace
    /// or as the source repository of a forked package.
    fn fork_destination(&self, destination: &Path, sources: &[&Installed]) -> Result<PathBuf> {
        ensure!(
            !destination
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            InvalidArgument,
            "use a destination without '..'"
        );
        let destination = std::path::absolute(destination)?;
        let root = fs::canonicalize(&self.root)?;
        let ancestor = destination.ancestors().find(|p| p.exists()).require(
            Error::InvalidArgument,
            "destination has no existing ancestor",
        )?;
        let real_ancestor = fs::canonicalize(ancestor)?;
        ensure!(
            !real_ancestor.starts_with(&root),
            InvalidArgument,
            "fork destination must be outside the managed workspace"
        );
        // The destination with symlinks resolved as far as it exists.
        let real = real_ancestor.join(destination.strip_prefix(ancestor).unwrap_or(Path::new("")));
        for p in sources {
            let Some(source) = Path::new(&p.source)
                .is_absolute()
                .then(|| fs::canonicalize(&p.source).ok())
                .flatten()
            else {
                continue;
            };
            ensure!(
                real != source,
                InvalidArgument,
                "fork destination {} is the source repository of the forked package; pass another folder",
                destination.display()
            );
            ensure!(
                !real.starts_with(&source),
                InvalidArgument,
                "fork destination {} is inside the source repository {}; pass a folder outside it",
                destination.display(),
                source.display()
            );
        }
        Ok(destination)
    }

    fn fork_packages_with(&self, ids: &[&str], destination: &Path) -> Result<PackageFork> {
        ensure!(!ids.is_empty(), InvalidArgument, "no package to fork");
        for id in ids {
            valid_id(id)?;
        }

        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        ensure!(
            unique.len() == ids.len(),
            InvalidArgument,
            "a package is listed more than once"
        );
        let _busy = self.busy();
        let state = self.state()?;
        let mut installed = Vec::new();
        for id in ids {
            let p = state
                .packages
                .get(*id)
                .ok_or_else(|| Error::NotInstalled { id: (*id).into() })?;
            installed.push((*id, p));
        }
        let sources: BTreeSet<&str> = installed.iter().map(|(_, p)| p.source.as_str()).collect();
        if sources.len() > 1 {
            let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
            for (id, package) in &installed {
                if let Some((_, ids)) = groups
                    .iter_mut()
                    .find(|(source, _)| *source == package.source)
                {
                    ids.push(id);
                } else {
                    groups.push((&package.source, vec![id]));
                }
            }
            let list: Vec<String> = groups
                .iter()
                .map(|(source, ids)| format!("{} from {source}", ids.join(", ")))
                .collect();
            fail!(
                InvalidArgument,
                "packages forked together must come from one repository; got {}",
                list.join("; ")
            );
        }
        // IDs identify a source repository, never a subset of its packages. Preserve
        // recorded revisions of all installed siblings, even when only one ID was passed.
        let source = installed[0].1.source.as_str();
        for (id, package) in &state.packages {
            if package.source == source && !installed.iter().any(|(seen, _)| *seen == id) {
                installed.push((id.as_str(), package));
            }
        }
        let (_, first_installed) = installed[0];
        let users: Vec<&Installed> = installed.iter().map(|(_, p)| *p).collect();
        let destination = self.fork_destination(destination, &users)?;
        ensure!(
            fs::symlink_metadata(&destination).is_err(),
            InvalidArgument,
            "destination already exists; pass a new folder"
        );
        clone_fork(
            &first_installed.source,
            &destination,
            self.options.custom_ssh,
            |destination| {
                let clone = destination;
                let mut sources = Vec::new();
                let mut new_ids: BTreeSet<String> = BTreeSet::new();
                let mut fresh_id = |original: &str| -> Result<String> {
                    let id = identity::new_id()?;
                    ensure!(
                        id != original
                            && !state.packages.contains_key(&id)
                            && !state.forgotten.contains_key(&id)
                            && new_ids.insert(id.clone()),
                        Workspace,
                        "generated fork UUID collides with an existing package"
                    );
                    Ok(id)
                };
                for (id, p) in &installed {
                    let new_id = fresh_id(id)?;
                    let commit = p.current.commit.clone();
                    if transport::require_object(
                        clone,
                        &format!("{commit}^{{commit}}"),
                        self.options.custom_ssh,
                    )
                    .is_err()
                    {
                        transport::fetch(
                            clone,
                            transport::Fetch::Revision {
                                remote: "origin",
                                revision: &commit,
                            },
                            self.options.custom_ssh,
                        )?;
                    }
                    let package = packages::find_at_with_ssh(
                        clone,
                        &commit,
                        Some(id),
                        self.options.custom_ssh,
                    )?;
                    sources.push(Source {
                        original: (*id).into(),
                        original_source: p.source.clone(),
                        id: new_id,
                        commit,
                        path: package.path,
                        base: package.manifest,
                    });
                }
                let base = fork_base(clone, &sources, self.options.custom_ssh)?;
                let siblings =
                    packages::packages_at_with_ssh(clone, &base, self.options.custom_ssh)?
                        .unwrap_or_default();
                for package in siblings {
                    let id = &package.manifest.id;
                    if sources.iter().any(|s| &s.original == id) {
                        continue;
                    }
                    let new_id = fresh_id(id)?;
                    sources.push(Source {
                        original: id.clone(),
                        original_source: first_installed.source.clone(),
                        id: new_id,
                        commit: base.clone(),
                        path: package.path,
                        base: package.manifest,
                    });
                }
                new_fork(destination, &sources, self.options.custom_ssh)
            },
        )
    }
}

/// Fork a Git path or URL without opening, creating or locking a workspace.
/// Every package receives a fresh UUID; IDs select and validate the source
/// revision, never a subset. With no IDs, the source must have a single package.
/// By default select the latest stable repository release containing the first ID;
/// every supplied ID must exist at that revision.
/// `reference` accepts any Git revision, including branches and untagged commits.
pub fn fork_source(
    source: &str,
    ids: &[&str],
    reference: Option<&str>,
    destination: &Path,
) -> Result<PackageFork> {
    fork_source_with(source, ids, reference, destination, &Options::default())
}

/// Source fork with explicit transport settings. No workspace settings are read.
pub fn fork_source_with(
    source: &str,
    ids: &[&str],
    reference: Option<&str>,
    destination: &Path,
    options: &Options,
) -> Result<PackageFork> {
    for id in ids {
        valid_id(id)?;
    }
    ensure!(
        ids.iter().copied().collect::<BTreeSet<_>>().len() == ids.len(),
        InvalidArgument,
        "a package is listed more than once"
    );
    let source = if Path::new(source).exists() {
        fs::canonicalize(source)?.to_string_lossy().into_owned()
    } else {
        source.to_owned()
    };
    crate::auth::validate_recorded(&source)?;
    ensure!(
        !destination
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        InvalidArgument,
        "use a destination without '..'"
    );
    let destination = std::path::absolute(destination)?;
    let ancestor = destination.ancestors().find(|p| p.exists()).require(
        Error::InvalidArgument,
        "destination has no existing ancestor",
    )?;
    let real = fs::canonicalize(ancestor)?.join(destination.strip_prefix(ancestor).unwrap());
    let local_source = local_source_path(&source)?;
    ensure!(
        !local_source.is_some_and(|source| real.starts_with(source)),
        InvalidArgument,
        "fork destination must be outside the source repository"
    );
    let ssh = options.custom_ssh;
    clone_fork(&source, &destination, ssh, |repo| {
        let commit = match reference {
            Some(reference) => resolve_source_ref(repo, reference, ssh)?,
            None => match releases::latest_with_ssh(repo, ids.first().copied(), ssh) {
                Ok(tag) => tag.commit,
                Err(Error::NoReleases(message)) => {
                    if let Some(id) = ids.first() {
                        let head = transport::object_id(repo, "HEAD", ssh)?;
                        packages::find_at_with_ssh(repo, &head, Some(id), ssh)?;
                    }
                    return Err(Error::NoReleases(message));
                }
                Err(error) => return Err(error),
            },
        };
        // Validate every supplied ID at the resolved revision; the package helper
        // reports available IDs on ambiguity or a missing package.
        if ids.is_empty() {
            packages::find_at_with_ssh(repo, &commit, None, ssh)?;
        }
        for id in ids {
            packages::find_at_with_ssh(repo, &commit, Some(id), ssh)?;
        }
        let mut packages = match packages::packages_at_with_ssh(repo, &commit, ssh)? {
            Some(packages) => packages,
            None => vec![packages::find_at_with_ssh(
                repo,
                &commit,
                ids.first().copied(),
                ssh,
            )?],
        };
        packages.sort_by_key(|p| {
            ids.iter()
                .position(|id| *id == p.manifest.id)
                .unwrap_or(ids.len())
        });
        let mut new_ids = BTreeSet::new();
        let mut sources = Vec::new();
        for package in packages {
            let original = package.manifest.id.clone();
            let id = identity::new_id()?;
            ensure!(
                id != original && new_ids.insert(id.clone()),
                Workspace,
                "generated fork UUID collides with an existing package"
            );
            sources.push(Source {
                original,
                original_source: source.clone(),
                id,
                commit: commit.clone(),
                path: package.path,
                base: package.manifest,
            });
        }
        new_fork(repo, &sources, ssh)
    })
}

/// A fresh clone has no stale local branches. Resolve normal Git expressions,
/// translating branch names to origin refs when they are not local branches.
fn resolve_source_ref(repo: &Path, reference: &str, custom_ssh: bool) -> Result<String> {
    ensure!(
        !reference.is_empty() && !reference.starts_with('-'),
        InvalidArgument,
        "invalid git ref"
    );
    if let Ok(commit) = resolve_ref(repo, reference, custom_ssh) {
        return Ok(commit);
    }
    let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
    if let Ok(commit) = resolve_ref(repo, &format!("refs/remotes/origin/{branch}"), custom_ssh) {
        return Ok(commit);
    }
    // A clone fetches heads and tags, but an explicit ref may live elsewhere.
    // Fetch its base name, then apply any ancestry expression to the fetched commit.
    let split = reference.find(['~', '^']).unwrap_or(reference.len());
    let (base, expression) = reference.split_at(split);
    transport::fetch(
        repo,
        transport::Fetch::Revision {
            remote: "origin",
            revision: base,
        },
        custom_ssh,
    )?;
    resolve_ref(repo, &format!("FETCH_HEAD{expression}"), custom_ssh)
}

/// Git treats file:// URLs as local paths, including their percent escapes.
/// Check their canonical path too, without changing the source URL recorded in metadata.
fn local_source_path(source: &str) -> Result<Option<PathBuf>> {
    if Path::new(source).is_absolute() {
        return Ok(Some(fs::canonicalize(source)?));
    }
    let Some(url) = source.strip_prefix("file://") else {
        return Ok(None);
    };
    let (_, path) = url
        .split_once('/')
        .require(Error::InvalidArgument, "file URL needs an absolute path")?;
    let mut decoded = Vec::new();
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex = |byte: Option<u8>| byte.and_then(|b| (b as char).to_digit(16));
            let high =
                hex(bytes.next()).require(Error::InvalidArgument, "invalid file URL escape")?;
            let low =
                hex(bytes.next()).require(Error::InvalidArgument, "invalid file URL escape")?;
            decoded.push((high * 16 + low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    let path = String::from_utf8(decoded)
        .map_err(|_| Error::InvalidArgument("file URL path must be UTF-8".into()))?;
    Ok(Some(fs::canonicalize(format!("/{path}"))?))
}

/// Clone a fresh repository and remove only newly created paths if preparation fails.
fn clone_fork(
    source: &str,
    destination: &Path,
    custom_ssh: bool,
    prepare: impl FnOnce(&Path) -> Result<PackageFork>,
) -> Result<PackageFork> {
    ensure!(
        fs::symlink_metadata(destination).is_err(),
        InvalidArgument,
        "destination already exists; pass a new folder"
    );
    crate::auth::validate_recorded(source)?;
    let parent = destination
        .parent()
        .require(Error::InvalidArgument, "destination needs a parent")?;
    let created = destination
        .ancestors()
        .take_while(|p| fs::symlink_metadata(p).is_err())
        .last()
        .unwrap_or(destination)
        .to_path_buf();
    let result = (|| {
        fs::create_dir_all(parent)?;
        transport::clone_repo(parent, source, &destination.to_string_lossy(), custom_ssh)?;
        prepare(destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&created);
    }
    result
}

/// Give the package manifest `m` (read from `source.path`) its fork identity.
fn identify(m: &mut Manifest, source: &Source) {
    m.id = source.id.clone();
    m.replaces = Some(Replaces {
        id: source.original.clone(),
        version: source.base.version.clone(),
    });
    m.fork_of = Some(ForkOf {
        id: source.original.clone(),
        version: source.base.version.clone(),
        commit: source.commit.clone(),
        // Only a source `check` may query (a URL or an absolute path, no remote helper).
        source: crate::auth::validate_recorded(&source.original_source)
            .is_ok()
            .then(|| source.original_source.clone()),
    });
}

/// Whether `path` has the same tree at both commits (`false` if it is missing in one).
fn same_folder(repo: &Path, a: &str, b: &str, path: &str, custom_ssh: bool) -> bool {
    let tree = |commit: &str| {
        transport::verify_revision(repo, &format!("{commit}:{path}"), true, custom_ssh).ok()
    };
    matches!((tree(a), tree(b)), (Some(x), Some(y)) if x == y)
}

/// Rebuild submodule configuration from the same revisions as the selected trees.
/// Keep repository-wide modules and modules of every package.
fn select_submodules(repo: &Path, base: &str, sources: &[Source], custom_ssh: bool) -> Result<()> {
    if sources.iter().any(|s| s.path.is_empty()) {
        return Ok(());
    }
    let folders = packages::layout_at_with_ssh(repo, base, custom_ssh)?.unwrap_or_default();
    let mut selected: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (commit, package) in std::iter::once((base, None)).chain(
        sources
            .iter()
            .map(|s| (s.commit.as_str(), Some(s.path.as_str()))),
    ) {
        let blob = format!("{commit}:.gitmodules");
        if transport::require_object(repo, &blob, custom_ssh).is_err() {
            continue;
        }
        let config = transport::config_blob(repo, &blob, custom_ssh)?;
        let mut modules: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for entry in config.split('\0').filter(|e| !e.is_empty()) {
            let Some((key, value)) = entry.split_once('\n') else {
                continue;
            };
            let Some(name) = key
                .strip_prefix("submodule.")
                .and_then(|key| key.rsplit_once('.').map(|(name, _)| name))
            else {
                continue;
            };
            modules
                .entry(name.to_owned())
                .or_default()
                .insert(key.to_owned(), value.to_owned());
        }
        for (name, values) in modules {
            let Some(path) = values.get(&format!("submodule.{name}.path")) else {
                continue;
            };
            let within = |folder: &str| path == folder || path.starts_with(&format!("{folder}/"));
            let keep = match package {
                Some(folder) => within(folder),
                None => !folders.folders().into_iter().any(within),
            };
            if keep {
                if let Some(previous) = selected.get(&name) {
                    ensure!(
                        previous == &values,
                        InvalidArgument,
                        "selected revisions have conflicting submodule configuration for {name}"
                    );
                }
                selected.insert(name, values);
            }
        }
    }
    let file = repo.join(".gitmodules");
    if file.exists() {
        fs::remove_file(&file)?;
    }
    for values in selected.values() {
        for (key, value) in values {
            transport::config_file_add(repo, ".gitmodules", key, value, custom_ssh)?;
        }
    }
    Ok(())
}

/// The newest installed revision must contain the other revisions in its history.
fn fork_base(repo: &Path, sources: &[Source], custom_ssh: bool) -> Result<String> {
    Ok(sources
        .iter()
        .map(|s| s.commit.as_str())
        .find(|candidate| {
            sources.iter().all(|s| {
                s.commit == *candidate
                    || transport::require_ancestor(repo, &s.commit, candidate, custom_ssh)
                        .is_ok()
            })
        })
        .require(
            Error::InvalidArgument,
            "the installed revisions of these packages are on diverged histories; update them to releases of one branch first",
        )?
        .to_owned())
}

fn new_fork(destination: &Path, sources: &[Source], custom_ssh: bool) -> Result<PackageFork> {
    let root = sources.iter().any(|s| s.path.is_empty());
    for source in sources {
        transport::require_object(
            destination,
            &format!("{}^{{commit}}", source.commit),
            custom_ssh,
        )
        .map_err(|_| {
            Error::Git(format!(
                "the installed revision {} of {} is missing from its clone",
                &source.commit[..source.commit.len().min(12)],
                source.original
            ))
        })?;
    }
    let base = fork_base(destination, sources, custom_ssh)?;
    // Renaming preserves remote branches and the URL used by relative submodules.
    transport::rename_remote(destination, "origin", "upstream", custom_ssh)?;
    let mut branch = "fork".to_owned();
    let mut number = 2;
    while !transport::branches_matching(destination, &branch, custom_ssh)?.is_empty()
        || transport::verify_revision(
            destination,
            &format!("refs/remotes/upstream/{branch}"),
            false,
            custom_ssh,
        )
        .is_ok()
    {
        branch = format!("fork-{number}");
        number += 1;
    }
    // Attached to the new branch, so the fork commit is on it (whatever HEAD was).
    transport::create_branch(destination, &branch, &base, custom_ssh)?;

    if !root {
        // A package installed at an older revision starts from that revision's files.
        for source in sources.iter().filter(|s| s.commit != base) {
            if same_folder(destination, &base, &source.commit, &source.path, custom_ssh) {
                continue;
            }
            transport::remove_cached_path(destination, &source.path, custom_ssh)?;
            match fs::remove_dir_all(destination.join(&source.path)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            transport::checkout_path(destination, &source.commit, &source.path, custom_ssh)?;
        }
    }
    select_submodules(destination, &base, sources, custom_ssh)?;
    transport::config_set(
        destination,
        &format!("branch.{branch}.remote"),
        "upstream",
        custom_ssh,
    )?;
    let mut manifests = Vec::new();
    for source in sources {
        manifests.push(package_manifest(destination, source.subdir())?);
    }
    for (m, source) in manifests.iter_mut().zip(sources) {
        identify(m, source);
        fs::write(
            destination.join(manifest_path(&source.path)),
            manifest_json(m)?,
        )?;
    }
    // A new repository created by gitpm: a local demo identity only if the user has none.
    scaffold::require_identity(destination, true, custom_ssh)?;
    // Commit all fork identities and the source trees.
    let message = fork_message(sources);
    // A new repository of gitpm's: user hooks are not run.
    transport::stage_all(destination, custom_ssh)?;
    transport::commit_without_hooks(destination, &message, custom_ssh)?;
    let commit = transport::object_id(destination, "HEAD", custom_ssh)?;
    if root {
        packages::find_at_with_ssh(destination, &commit, Some(&sources[0].id), custom_ssh)?;
    } else {
        // The fork must be a valid multi-package repository with exactly these packages.
        let found =
            packages::packages_at_with_ssh(destination, &commit, custom_ssh)?.unwrap_or_default();
        ensure!(
            found.len() == sources.len()
                && sources
                    .iter()
                    .all(|s| found.iter().any(|p| p.manifest.id == s.id)),
            InvalidManifest,
            "the fork does not list exactly the forked packages"
        );
    }
    Ok(PackageFork {
        path: fs::canonicalize(destination)?,
        created: true,
        commit,
        packages: forked(sources, &manifests),
    })
}

fn fork_message(sources: &[Source]) -> String {
    let originals: Vec<&str> = sources.iter().map(|s| s.original.as_str()).collect();
    let forks: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
    format!("Fork {} as {}", originals.join(", "), forks.join(", "))
}

fn forked(sources: &[Source], manifests: &[Manifest]) -> Vec<ForkedPackage> {
    sources
        .iter()
        .zip(manifests)
        .map(|(s, m)| ForkedPackage {
            original: s.original.clone(),
            id: s.id.clone(),
            path: s.path.clone(),
            version: m.version.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_source_retains_all_packages_at_resolved_revision_without_workspace() {
        use crate::test_support::{TempDir, git, manifest, write_manifest};
        let temp = TempDir::new("fork-source");
        let repo = temp.join("source");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("gitpm.json"), r#"{"packages":["packages/*"]}"#).unwrap();
        fs::write(repo.join("README.md"), "Repository files survive").unwrap();
        for name in ["theme", "panel"] {
            let folder = repo.join(format!("packages/{name}"));
            fs::create_dir_all(folder.join("dist")).unwrap();
            fs::write(folder.join("dist/index.js"), name).unwrap();
            let mut m = manifest(
                if name == "theme" {
                    "11111111-1111-4111-8111-111111111111"
                } else {
                    "22222222-2222-4222-8222-222222222222"
                },
                "1.0.0",
            );
            if name == "panel" {
                m.dependencies
                    .insert("11111111-1111-4111-8111-111111111111".into(), "^1".into());
            }
            write_manifest(&folder, &m);
        }
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "initial"]);
        git(&repo, &["tag", "v1.0.0"]);
        let released = git(&repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("README.md"), "Unreleased commit").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "unreleased"]);
        let head = git(&repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("README.md"), "Uncommitted work").unwrap();
        for (name, reference, expected) in [
            ("latest", None, &released),
            ("explicit", Some("main"), &head),
        ] {
            let dest = temp.join(name);
            let fork = fork_source(
                repo.to_str().unwrap(),
                &["11111111-1111-4111-8111-111111111111"],
                reference,
                &dest,
            )
            .unwrap();
            assert_eq!(fork.packages.len(), 2);
            let panel = package_manifest(&dest, Some("packages/panel")).unwrap();
            assert_eq!(
                panel.dependencies["11111111-1111-4111-8111-111111111111"],
                "^1"
            );
            let before = crate::repository_packages(&dest, None).unwrap();
            let core = before.iter().find(|p| p.path == "packages/theme").unwrap();
            let panel = before.iter().find(|p| p.path == "packages/panel").unwrap();
            let edited = crate::edit_dependency(
                &dest,
                &panel.id,
                &crate::DependencyEdit::Update {
                    dependency: "11111111-1111-4111-8111-111111111111".into(),
                    new_dependency: core.id.clone(),
                    requirement: "^1".into(),
                },
            )
            .unwrap();
            assert_eq!(edited.dependencies[&core.id], "^1");
            for package in &fork.packages {
                let m = package_manifest(&dest, Some(&package.path)).unwrap();
                valid_id(&m.id).unwrap();
                assert_ne!(m.id, package.original);
                assert_eq!(
                    m.name,
                    package_manifest(&repo, Some(&package.path)).unwrap().name
                );
                assert_eq!(m.fork_of.unwrap().commit, *expected);
            }
            assert_eq!(git(&dest, &["rev-parse", "HEAD^"]), *expected);
            assert_eq!(
                fs::read(dest.join("gitpm.json")).unwrap(),
                fs::read(repo.join("gitpm.json")).unwrap()
            );
            assert_ne!(
                fs::read_to_string(dest.join("README.md")).unwrap(),
                "Uncommitted work"
            );
        }
        for ids in [
            vec![],
            vec!["33333333-3333-4333-8333-333333333333"],
            vec![
                "11111111-1111-4111-8111-111111111111",
                "11111111-1111-4111-8111-111111111111",
            ],
        ] {
            let dest = temp.join("failed/nested/fork");
            let e = fork_source(repo.to_str().unwrap(), &ids, None, &dest).unwrap_err();
            assert_eq!(e.kind(), "invalid_argument");
            assert!(!temp.join("failed").exists());
        }
        assert!(!temp.join("workspace").exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
    }

    #[test]
    fn fork_source_ignores_newer_tags_without_id_and_legacy_tags() {
        use crate::test_support::{Author, TempDir, git};
        let temp = TempDir::new("fork-source-releases");
        let author = Author::new(temp.join("source"), "11111111-1111-4111-8111-111111111111");
        let released = author.head();
        let untagged = author.commit("2.0.0");
        git(
            &author.path,
            &["tag", "11111111-1111-4111-8111-111111111111@2.0.0"],
        );
        let mut m = package_manifest(&author.path, None).unwrap();
        m.id = "44444444-4444-4444-8444-444444444444".into();
        crate::test_support::write_manifest(&author.path, &m);
        git(&author.path, &["add", "."]);
        git(&author.path, &["commit", "-qm", "rename"]);
        git(&author.path, &["tag", "v2.0.0"]);
        let dest = temp.join("fork");
        fork_source(
            author.source(),
            &["11111111-1111-4111-8111-111111111111"],
            None,
            &dest,
        )
        .unwrap();
        assert_eq!(
            package_manifest(&dest, None)
                .unwrap()
                .fork_of
                .unwrap()
                .commit,
            released
        );
        let dest = temp.join("by-commit");
        fork_source(
            author.source(),
            &["11111111-1111-4111-8111-111111111111"],
            Some(&untagged),
            &dest,
        )
        .unwrap();
        assert_eq!(
            package_manifest(&dest, None)
                .unwrap()
                .fork_of
                .unwrap()
                .commit,
            untagged
        );
        fs::create_dir_all(temp.join("in-source")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&author.path, temp.join("in-source/link")).unwrap();
            assert!(
                fork_source(
                    author.source(),
                    &["11111111-1111-4111-8111-111111111111"],
                    None,
                    &temp.join("in-source/link/fork")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn fork_source_accepts_revision_expressions_and_nondefault_branches() {
        use crate::test_support::{Author, TempDir, git};
        let temp = TempDir::new("fork-source-expressions");
        let author = Author::new(temp.join("source"), "11111111-1111-4111-8111-111111111111");
        let first = author.head();
        author.commit("0.2.0");
        git(&author.path, &["branch", "topic"]);
        for (i, reference) in ["HEAD~1", "topic~1", "refs/heads/topic~1", "v0.1.0^{commit}"]
            .iter()
            .enumerate()
        {
            let dest = temp.join(format!("fork-{i}"));
            fork_source(
                author.source(),
                &["11111111-1111-4111-8111-111111111111"],
                Some(reference),
                &dest,
            )
            .unwrap();
            assert_eq!(
                package_manifest(&dest, None)
                    .unwrap()
                    .fork_of
                    .unwrap()
                    .commit,
                first
            );
        }
    }

    #[test]
    fn fork_source_fetches_explicit_refs_outside_heads_and_tags() {
        use crate::test_support::{Author, TempDir, git};
        let temp = TempDir::new("fork-source-custom-ref");
        let author = Author::new(temp.join("source"), "11111111-1111-4111-8111-111111111111");
        git(&author.path, &["checkout", "--orphan", "temporary"]);
        let commit = author.commit("0.2.0");
        git(&author.path, &["update-ref", "refs/custom/review", &commit]);
        git(&author.path, &["checkout", "main"]);
        git(&author.path, &["branch", "-D", "temporary"]);
        let dest = temp.join("fork");
        fork_source(
            author.source(),
            &["11111111-1111-4111-8111-111111111111"],
            Some("refs/custom/review"),
            &dest,
        )
        .unwrap();
        assert_eq!(
            package_manifest(&dest, None)
                .unwrap()
                .fork_of
                .unwrap()
                .commit,
            commit
        );
    }

    #[test]
    fn fork_source_file_url_cannot_create_destination_inside_source() {
        use crate::test_support::{Author, TempDir, git};
        let temp = TempDir::new("fork-source-file-url");
        let author = Author::new(
            temp.join("source with spaces"),
            "11111111-1111-4111-8111-111111111111",
        );
        let before = git(&author.path, &["status", "--porcelain"]);
        for source in [
            format!("file://{}", author.source()),
            format!("file://localhost{}", author.source().replace(' ', "%20")),
        ] {
            let dest = author.path.join("nested/fork");
            let error = fork_source(
                &source,
                &["11111111-1111-4111-8111-111111111111"],
                None,
                &dest,
            )
            .unwrap_err();
            assert_eq!(error.kind(), "invalid_argument");
            assert!(!author.path.join("nested").exists());
        }
        assert_eq!(git(&author.path, &["status", "--porcelain"]), before);
        let url = format!("file://localhost{}", author.source().replace(' ', "%20"));
        let dest = temp.join("outside-source");
        fork_source(&url, &["11111111-1111-4111-8111-111111111111"], None, &dest).unwrap();
        assert_eq!(
            package_manifest(&dest, None)
                .unwrap()
                .fork_of
                .unwrap()
                .source
                .as_deref(),
            Some(url.as_str())
        );
    }
}
