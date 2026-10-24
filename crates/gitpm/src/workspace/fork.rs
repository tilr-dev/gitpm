//! Fork installed packages by cloning their recorded source at installed revisions.
use super::*;
use std::collections::BTreeSet;

/// A dependency of a package rewritten by a fork: `from` (an original) became `to` (its
/// fork). `new_requirement` is set when `requirement` does not accept the fork's version
/// and was replaced by `^VERSION`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DependencyRewrite {
    /// Package (its ID in the fork repository) whose `plugin.json` changed.
    pub package: String,
    pub from: String,
    pub to: String,
    pub requirement: String,
    #[serde(rename = "newRequirement", skip_serializing_if = "Option::is_none")]
    pub new_requirement: Option<String>,
}
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
    pub rewrites: Vec<DependencyRewrite>,
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

/// Rewrite the dependencies of `m` that name a key of `forks` to the fork (ID, version
/// declared by the fork); a requirement that does not accept that version becomes
/// `^VERSION`. A manifest that already depends on the fork too (or on two originals of
/// one fork) is refused, whatever the order of its keys.
fn rewrite_dependencies(
    m: &mut Manifest,
    forks: &BTreeMap<String, (String, String)>,
    rewrites: &mut Vec<DependencyRewrite>,
) -> Result<()> {
    let original = std::mem::take(&mut m.dependencies);
    let mut dependencies = BTreeMap::new();
    for (dependency, requirement) in &original {
        let Some((fork, version)) = forks.get(dependency) else {
            dependencies.insert(dependency.clone(), requirement.clone());
            continue;
        };
        ensure!(
            !original.contains_key(fork),
            InvalidManifest,
            "{} depends on both {dependency} and its fork {fork}; remove one of them first",
            m.id
        );
        if let Some((other, _)) = original
            .keys()
            .filter(|other| *other != dependency)
            .find_map(|other| forks.get(other).map(|f| (other, f)))
            .filter(|(_, (f, _))| f == fork)
        {
            fail!(
                InvalidManifest,
                "{} depends on both {dependency} and {other}, which are both forked as {fork}",
                m.id
            );
        }
        let new_requirement = (!deps::matches(requirement, version)).then(|| format!("^{version}"));
        rewrites.push(DependencyRewrite {
            package: m.id.clone(),
            from: dependency.clone(),
            to: fork.clone(),
            requirement: requirement.clone(),
            new_requirement: new_requirement.clone(),
        });
        dependencies.insert(
            fork.clone(),
            new_requirement.unwrap_or_else(|| requirement.clone()),
        );
    }
    m.dependencies = dependencies;
    Ok(())
}

impl Manager {
    /// Fork one installed package as `NAMESPACE/<same name>`: [`Manager::fork_packages`]
    /// with one ID. Returns the fork repository.
    pub fn fork_in_namespace(
        &self,
        id: &str,
        destination: &Path,
        namespace: &str,
    ) -> Result<PathBuf> {
        Ok(self.fork_packages(&[id], destination, namespace)?.path)
    }
    /// Fork installed packages of one repository into `namespace` (new IDs
    /// `NAMESPACE/<same name>`) as the fork repository `destination`:
    ///
    /// The destination must be a new folder outside the workspace. Clone the recorded
    /// source with history on a new `fork` branch, retaining submodule Git links.
    /// Installed packages start at their recorded revisions; local installation work
    /// is ignored. Other packages start at the newest installed revision. All packages in the repository are renamed, including siblings
    /// that are not installed; repository files and the package layout are preserved.
    /// Each manifest gets its new ID, `forkOf` and `replaces`; dependencies among
    /// packages forked together are rewritten. Nothing is tagged.
    pub fn fork_packages(
        &self,
        ids: &[&str],
        destination: &Path,
        namespace: &str,
    ) -> Result<PackageFork> {
        self.fork_packages_with(ids, destination, namespace)
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

    fn fork_packages_with(
        &self,
        ids: &[&str],
        destination: &Path,
        namespace: &str,
    ) -> Result<PackageFork> {
        ensure!(!ids.is_empty(), InvalidArgument, "no package to fork");
        for id in ids {
            valid_id(id)?;
        }
        identity::valid_part(namespace)?;
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
        crate::auth::validate_recorded(&first_installed.source)?;
        let parent = destination
            .parent()
            .require(Error::InvalidArgument, "destination needs a parent")?;
        let created = destination
            .ancestors()
            .take_while(|p| fs::symlink_metadata(p).is_err())
            .last()
            .unwrap_or(&destination)
            .to_path_buf();
        let result = (|| {
            fs::create_dir_all(parent)?;
            crate::transport::clone_repo(
                parent,
                &first_installed.source,
                &destination.to_string_lossy(),
                self.options.custom_ssh,
            )?;
            let clone = &destination;
            let mut sources = Vec::new();
            let mut new_ids: BTreeSet<String> = BTreeSet::new();
            let mut qualify = |id: &str| -> Result<String> {
                let new_id = identity::qualified(namespace, id.rsplit('/').next().unwrap())?;
                ensure!(
                    new_id != id,
                    InvalidArgument,
                    "fork needs a different namespace; pass another namespace"
                );
                ensure!(
                    !new_ids.iter().any(|other| ids_collide(other, &new_id))
                        && new_ids.insert(new_id.clone()),
                    InvalidArgument,
                    "two packages would get the same fork ID {new_id}"
                );
                ensure!(
                    !ids_collide(&new_id, id)
                        && !state
                            .packages
                            .keys()
                            .chain(state.forgotten.keys())
                            .any(|other| ids_collide(other, &new_id)),
                    InvalidArgument,
                    "fork ID {new_id} differs only in letter case from an installed package"
                );
                Ok(new_id)
            };
            for (id, p) in &installed {
                let new_id = qualify(id)?;
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
                let package =
                    packages::find_at_with_ssh(clone, &commit, Some(id), self.options.custom_ssh)?;
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
            let siblings = packages::packages_at_with_ssh(clone, &base, self.options.custom_ssh)?
                .unwrap_or_default();
            for package in siblings {
                let id = &package.manifest.id;
                if sources.iter().any(|s| &s.original == id) {
                    continue;
                }
                let new_id = qualify(id)?;
                sources.push(Source {
                    original: id.clone(),
                    original_source: first_installed.source.clone(),
                    id: new_id,
                    commit: base.clone(),
                    path: package.path,
                    base: package.manifest,
                });
            }
            new_fork(&destination, &sources, self.options.custom_ssh)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&created);
        }
        result
    }
}

/// The forks a dependency may be rewritten to: original ID → (fork ID, declared version).
fn fork_map(sources: &[Source], manifests: &[Manifest]) -> BTreeMap<String, (String, String)> {
    sources
        .iter()
        .zip(manifests)
        .map(|(s, m)| (s.original.clone(), (s.id.clone(), m.version.clone())))
        .collect()
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
    let forks = fork_map(sources, &manifests);
    let mut rewrites = Vec::new();
    for (m, source) in manifests.iter_mut().zip(sources) {
        identify(m, source);
        rewrite_dependencies(m, &forks, &mut rewrites)?;
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
        rewrites,
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

    fn with_dependencies(id: &str, dependencies: &[(&str, &str)]) -> Manifest {
        Manifest {
            id: id.into(),
            version: "1.0.0".into(),
            dependencies: dependencies
                .iter()
                .map(|(id, requirement)| ((*id).into(), (*requirement).into()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order() {
        // B9: the fork ID sorts after (zz) and before (aa) the original.
        for namespace in ["zz", "aa"] {
            let fork = format!("{namespace}/theme");
            let forks =
                BTreeMap::from([("tilr/theme".to_owned(), (fork.clone(), "1.0.0".to_owned()))]);
            let mut m =
                with_dependencies("me/panel", &[("tilr/theme", "^1"), (fork.as_str(), "^2")]);
            let mut rewrites = Vec::new();
            let error = rewrite_dependencies(&mut m, &forks, &mut rewrites).unwrap_err();
            assert_eq!(error.kind(), "invalid_manifest", "{namespace}");
            assert!(
                error
                    .to_string()
                    .contains(&format!("depends on both tilr/theme and its fork {fork}")),
                "{error}"
            );
        }
        // Two originals of one fork (forkOf and replaces of an existing package).
        let forks = BTreeMap::from([
            (
                "a/theme".to_owned(),
                ("me/theme".to_owned(), "1.0.0".to_owned()),
            ),
            (
                "b/theme".to_owned(),
                ("me/theme".to_owned(), "1.0.0".to_owned()),
            ),
        ]);
        let mut m = with_dependencies("me/panel", &[("a/theme", "^1"), ("b/theme", "^1")]);
        let error = rewrite_dependencies(&mut m, &forks, &mut Vec::new()).unwrap_err();
        assert!(
            error.to_string().contains("both forked as me/theme"),
            "{error}"
        );
        // The plain case: rewritten, with a new requirement when needed.
        let forks = BTreeMap::from([(
            "tilr/theme".to_owned(),
            ("me/theme".to_owned(), "2.0.0".to_owned()),
        )]);
        let mut m = with_dependencies("me/panel", &[("tilr/theme", "^1"), ("x/y", "*")]);
        let mut rewrites = Vec::new();
        rewrite_dependencies(&mut m, &forks, &mut rewrites).unwrap();
        assert_eq!(m.dependencies["me/theme"], "^2.0.0");
        assert_eq!(m.dependencies["x/y"], "*");
        assert_eq!(rewrites[0].new_requirement.as_deref(), Some("^2.0.0"));
    }
}
