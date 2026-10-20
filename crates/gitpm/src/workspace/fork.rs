//! Forks: installed packages of one repository become packages of a fork repository
//! with new IDs `NAMESPACE/<same name>`. The fork repository is given explicitly; it is
//! never derived from a package ID or a source. A new fork is a copy of the source
//! clone (history and local edits), reduced to the forked packages; an existing
//! multi-package repository gets the package folders added. A single fork is the
//! one-package case of the same code. See [`Manager::fork_packages`].
use super::*;
use std::collections::{BTreeSet, HashMap};

/// Step of a fork with `--replace` ([`Manager::fork_packages_replace`]) that failed after
/// the fork repository was created or changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForkStep {
    /// Committing/tagging the fork's first release.
    Release,
    /// Installing the released fork into the workspace.
    Install,
    /// Making the installed fork the provider of the original.
    Replace,
}
impl ForkStep {
    pub fn name(self) -> &'static str {
        match self {
            ForkStep::Release => "release",
            ForkStep::Install => "install",
            ForkStep::Replace => "replace",
        }
    }
}
/// What [`Manager::fork_packages_replace`] did for one forked package. The fork
/// repository is never removed: a failure is reported in `failed`/`error` with what was
/// done so far.
#[derive(Clone, Debug, Serialize)]
pub struct ForkReplacement {
    pub original: String,
    pub id: String,
    /// The fork repository (author folder, outside the workspace).
    pub path: PathBuf,
    /// Release tag of the fork, once created.
    pub tag: Option<String>,
    pub installed: Option<InstallResult>,
    pub replacement: Option<ReplacementPlan>,
    pub failed: Option<ForkStep>,
    #[serde(skip)]
    pub error: Option<Error>,
    /// The replace step needs confirmation (incompatible consumers, not confirmed): the
    /// plan to show; [`Manager::fork_replace_resume`] applies it once confirmed. `failed`
    /// is then `Replace` and `error` the `incompatible_replacement` refusal.
    pub pending: Option<ReplacementPlan>,
}
/// Result of [`Manager::fork_replace_resume`]: the replacements applied (in order, each
/// with its dist report) and the first one that failed, if any.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ForkResume {
    pub applied: Vec<ReplacementPlan>,
    pub failed: Option<ResumeFailure>,
}
/// The pending replacement [`Manager::fork_replace_resume`] stopped at.
#[derive(Clone, Debug, Serialize)]
pub struct ResumeFailure {
    pub original: String,
    pub fork: String,
    #[serde(skip)]
    pub error: Error,
}
/// A replacement of `fork --replace` waiting for confirmation (see
/// [`ForkReplacement::pending`]), as [`Manager::fork_replace_resume`] takes it.
#[derive(Clone, Debug, Serialize)]
pub struct PendingReplacement {
    pub original: String,
    pub fork: String,
    /// The plan that was shown for confirmation.
    pub plan: ReplacementPlan,
}

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
    /// `true`: a new repository; `false`: packages were added to an existing one.
    pub created: bool,
    /// The commit that created or added the packages.
    pub commit: String,
    pub packages: Vec<ForkedPackage>,
    pub rewrites: Vec<DependencyRewrite>,
}
/// Result of [`Manager::fork_packages_replace`]: the fork plus one [`ForkReplacement`]
/// per forked package (in install order) with what was done for it.
#[derive(Clone, Debug, Serialize)]
pub struct PackageForkReplacement {
    pub fork: PackageFork,
    pub packages: Vec<ForkReplacement>,
}
impl PackageForkReplacement {
    /// The first failure (by package order), if any package did not finish.
    pub fn error(&self) -> Option<&Error> {
        self.packages.iter().find_map(|p| p.error.as_ref())
    }
    /// The replacements waiting for confirmation, in package order.
    pub fn pending(&self) -> Vec<PendingReplacement> {
        self.packages
            .iter()
            .filter_map(|p| {
                p.pending.as_ref().map(|plan| PendingReplacement {
                    original: p.original.clone(),
                    fork: p.id.clone(),
                    plan: plan.clone(),
                })
            })
            .collect()
    }
}

/// One package to fork: where it comes from and what it becomes.
struct Source {
    original: String,
    /// Recorded source of the original (`packages.<original>.source`).
    original_source: String,
    id: String,
    /// Revision the fork starts the package from: the installed commit (for an own
    /// clone its HEAD, which may hold local commits on top of it).
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

/// A package folder as a pathspec (`.` for the root package).
fn pathspec(folder: &str) -> &str {
    if folder.is_empty() { "." } else { folder }
}
fn manifest_path(folder: &str) -> String {
    if folder.is_empty() {
        "plugin.json".to_owned()
    } else {
        format!("{folder}/plugin.json")
    }
}

/// Rewrite the dependencies of `m` that name a key of `forks` to the fork (ID, version
/// the fork is released with); a requirement that does not accept that version becomes
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

/// Paths that differ from HEAD (staged, unstaged or untracked), limited to `paths`.
fn dirty(repo: &Path, paths: &[String], custom_ssh: bool) -> Result<Vec<String>> {
    let specs: Vec<&str> = paths.iter().map(|p| pathspec(p)).collect();
    lost_work::changed_paths(repo, &specs, custom_ssh)
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
    /// [`Manager::fork_packages_replace`] with one ID: fork, release the fork right away
    /// (tag `NAMESPACE/NAME@VERSION`; `version` defaults to the forked version, local
    /// edits of the package are committed with it), install it and make it the provider
    /// of `id` for every consumer ([`Manager::replace`] with `confirm`).
    pub fn fork_replace(
        &self,
        id: &str,
        destination: &Path,
        namespace: &str,
        version: Option<&str>,
        confirm: bool,
    ) -> Result<ForkReplacement> {
        let mut done =
            self.fork_packages_replace(&[id], destination, namespace, version, confirm)?;
        Ok(done.packages.remove(0))
    }

    /// Fork installed packages of one repository into `namespace` (new IDs
    /// `NAMESPACE/<same name>`) as the fork repository `destination`:
    ///
    /// - `destination` does not exist: a copy of the source clone with history and local
    ///   edits, on a new `fork` branch, the original repository as `upstream` remote.
    ///   Each package starts from its installed revision (an own clone from its HEAD).
    ///   For packages of a multi-package repository the folders of packages that are
    ///   not forked are removed and `gitpm.json` lists exactly the forked folders; a root
    ///   package (no `gitpm.json`) stays the root of the fork. One commit records the
    ///   change; local edits stay uncommitted.
    /// - `destination` is a multi-package repository (`gitpm.json`): the package folders
    ///   (files of the installed revisions) are added at their original folders and to
    ///   `gitpm.json` in one commit; only these paths are committed.
    /// - anything else is refused, as is a destination inside the workspace, with `..`,
    ///   or the source repository itself.
    ///
    /// Each `plugin.json` gets the new ID, `forkOf` and `replaces`. A dependency on a
    /// package forked here, or (when adding) on an original that a package of the
    /// repository forks (`forkOf`/`replaces`), is rewritten to the fork; existing packages
    /// that depend on a newly forked original are rewritten too. A requirement that does
    /// not accept the fork's version becomes `^VERSION`. Nothing is tagged.
    pub fn fork_packages(
        &self,
        ids: &[&str],
        destination: &Path,
        namespace: &str,
    ) -> Result<PackageFork> {
        self.fork_packages_with(ids, destination, namespace, None, false)
    }

    /// [`Manager::fork_packages`], then release every forked package at once (one commit
    /// of the changes in their folders, `version` set in each manifest when given, one
    /// `ID@VERSION` tag per package), install them (dependencies first) and replace each
    /// original for every consumer (`confirm` as in [`Manager::replace`]). Errors before
    /// the fork exists are returned as errors and leave nothing behind; after that the
    /// fork is kept and every package reports its own `failed` step and `error`. A failed
    /// install or replace of one package does not stop the others.
    pub fn fork_packages_replace(
        &self,
        ids: &[&str],
        destination: &Path,
        namespace: &str,
        version: Option<&str>,
        confirm: bool,
    ) -> Result<PackageForkReplacement> {
        if let Some(version) = version {
            catalog::version_key(version)?;
        }
        let fork = self.fork_packages_with(ids, destination, namespace, version, true)?;
        // Dependencies first (a cycle was refused before the fork was created).
        let edges: Vec<(&str, BTreeSet<String>)> = fork
            .packages
            .iter()
            .map(|p| {
                let subdir = (!p.path.is_empty()).then_some(p.path.as_str());
                let deps = package_manifest(&fork.path, subdir)
                    .map(|m| m.dependencies.into_keys().collect())
                    .unwrap_or_default();
                (p.id.as_str(), deps)
            })
            .collect();
        let order = install_order(&edges).unwrap_or_else(|_| (0..edges.len()).collect());
        let mut results: Vec<ForkReplacement> = order
            .into_iter()
            .map(|index| {
                let package = &fork.packages[index];
                ForkReplacement {
                    original: package.original.clone(),
                    id: package.id.clone(),
                    path: fork.path.clone(),
                    tag: None,
                    installed: None,
                    replacement: None,
                    failed: None,
                    error: None,
                    pending: None,
                }
            })
            .collect();
        if let Err((tagged, error)) = release_packages(&fork, version, self.options.custom_ssh) {
            // The packages are released together: nothing is installed unless all are.
            for result in &mut results {
                result.tag = tagged.get(&result.id).cloned();
                result.failed = Some(ForkStep::Release);
                result.error = Some(error.clone());
            }
            return Ok(PackageForkReplacement {
                fork,
                packages: results,
            });
        }
        let source = fork.path.to_string_lossy().into_owned();
        for result in &mut results {
            let version = version.map(str::to_owned).unwrap_or_else(|| {
                fork.packages
                    .iter()
                    .find(|p| p.id == result.id)
                    .map(|p| p.version.clone())
                    .unwrap_or_default()
            });
            result.tag = Some(identity::release_tag(&result.id, &version));
            match self.install_package(&source, Some(&result.id), InstallTarget::Release) {
                Ok(installed) => result.installed = Some(installed),
                Err(error) => {
                    result.failed = Some(ForkStep::Install);
                    result.error = Some(error);
                }
            }
        }
        for result in results.iter_mut().filter(|r| r.failed.is_none()) {
            match self.replace(&result.original, &result.id, confirm) {
                Ok(plan) => result.replacement = Some(plan),
                Err(error) => {
                    if let Error::IncompatibleReplacement { .. } = error {
                        result.pending = self.replace_plan(&result.original, &result.id).ok();
                    }
                    result.failed = Some(ForkStep::Replace);
                    result.error = Some(error);
                }
            }
        }
        Ok(PackageForkReplacement {
            fork,
            packages: results,
        })
    }

    /// Apply the pending replacements of a `fork --replace` (see
    /// [`PackageForkReplacement::pending`]) once the caller confirmed them (`confirm`;
    /// unconfirmed, an incompatible one is refused as by [`Manager::replace`]). Each is
    /// planned again under the lock, one transaction per package, in order: when its
    /// incompatible consumers are no longer the confirmed ones it is refused with
    /// [`Error::IncompatibleReplacement`]. The first failure stops the run; the result
    /// lists what was applied (each plan with its dist report) and that failure, and the
    /// replacements after it stay pending.
    pub fn fork_replace_resume(&self, pending: &[PendingReplacement], confirm: bool) -> ForkResume {
        let mut done = ForkResume::default();
        for item in pending {
            let applied = self.transaction(|_, state| {
                let plan = replacement_plan(state, &item.original, Some(&item.fork))?;
                if plan.incompatible != item.plan.incompatible {
                    return Err(Error::IncompatibleReplacement {
                        message: format!(
                            "the consumers of {} that {} does not satisfy changed since the replacement was confirmed",
                            item.original, item.fork
                        ),
                        id: item.original.clone(),
                        consumers: plan.incompatible.iter().map(|c| c.id.clone()).collect(),
                    });
                }
                plan.refuse_unless(confirm)?;
                state
                    .replacements
                    .insert(item.original.clone(), item.fork.clone());
                Ok(plan)
            });
            match applied {
                Ok((plan, dist)) => done.applied.push(ReplacementPlan { dist, ..plan }),
                Err(error) => {
                    done.failed = Some(ResumeFailure {
                        original: item.original.clone(),
                        fork: item.fork.clone(),
                        error,
                    });
                    break;
                }
            }
        }
        done
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

    /// `version`: the version the forks will be released with (for requirement checks).
    /// `install`: the forks will be installed one after another, so a dependency cycle
    /// between them is refused before anything is created.
    fn fork_packages_with(
        &self,
        ids: &[&str],
        destination: &Path,
        namespace: &str,
        version: Option<&str>,
        install: bool,
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
        let clones: BTreeSet<String> = installed.iter().map(|(id, p)| p.clone_folder(id)).collect();
        if clones.len() > 1 {
            // One entry per source ("a, b from X; c from Y"); one source installed into
            // several clones (an own clone and a shared one) names them.
            let mut groups: Vec<(&str, Vec<&str>, BTreeSet<String>)> = Vec::new();
            for (id, p) in &installed {
                let index = match groups.iter().position(|g| g.0 == p.source) {
                    Some(index) => index,
                    None => {
                        groups.push((p.source.as_str(), Vec::new(), BTreeSet::new()));
                        groups.len() - 1
                    }
                };
                groups[index].1.push(id);
                groups[index].2.insert(p.clone_folder(id));
            }
            let list: Vec<String> = groups
                .iter()
                .map(|(source, ids, folders)| {
                    let mut item = format!("{} from {source}", ids.join(", "));
                    if folders.len() > 1 {
                        let folders: Vec<&str> = folders.iter().map(String::as_str).collect();
                        item.push_str(&format!(
                            " (installed in separate clones {})",
                            folders.join(", ")
                        ));
                    }
                    item
                })
                .collect();
            fail!(
                InvalidArgument,
                "packages forked together must come from one repository; got {}",
                list.join("; ")
            );
        }
        let (first, first_installed) = installed[0];
        let clone = installed_repo(&self.root, first, first_installed)?;
        // An own clone is forked from its HEAD: local commits on top of the installed
        // revision belong to the fork. A shared clone's HEAD can be another package's
        // revision, so each package starts from its installed one.
        let own_head = match first_installed.clone {
            None => Some(git(
                &clone,
                &["rev-parse", "HEAD"],
                self.options.custom_ssh,
            )?),
            Some(_) => None,
        };
        let mut sources = Vec::new();
        let mut new_ids = BTreeSet::new();
        for (id, p) in &installed {
            let new_id = identity::qualified(namespace, id.rsplit('/').next().unwrap())?;
            ensure!(
                new_id != *id,
                InvalidArgument,
                "fork needs a different namespace; pass another namespace"
            );
            ensure!(
                new_ids.insert(new_id.clone()),
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
            let commit = own_head.clone().unwrap_or_else(|| p.current.commit.clone());
            let package = match packages::find_at_with_ssh(
                &clone,
                &commit,
                Some(id),
                self.options.custom_ssh,
            ) {
                Ok(package) => package,
                Err(Error::InvalidArgument(_)) if own_head.is_some() => fail!(
                    InvalidManifest,
                    "installed repository HEAD has a different package ID"
                ),
                Err(error) => return Err(error),
            };
            sources.push(Source {
                original: (*id).into(),
                original_source: p.source.clone(),
                id: new_id,
                commit,
                path: package.path,
                base: package.manifest,
            });
        }
        if install {
            // Dependencies between the forks are those between their originals.
            let edges: Vec<(&str, BTreeSet<String>)> = sources
                .iter()
                .map(|s| {
                    (
                        s.original.as_str(),
                        s.base.dependencies.keys().cloned().collect(),
                    )
                })
                .collect();
            install_order(&edges)?;
        }
        let users: Vec<&Installed> = installed.iter().map(|(_, p)| *p).collect();
        let destination = self.fork_destination(destination, &users)?;
        if fs::symlink_metadata(&destination).is_ok() {
            ensure!(
                destination.is_dir() && destination.join(packages::CONFIG).is_file(),
                InvalidArgument,
                "destination already exists and is not a multi-package repository ({}); pass a new folder or a repository with {}",
                packages::CONFIG,
                packages::CONFIG
            );
            if let Some(root) = sources.iter().find(|s| s.path.is_empty()) {
                fail!(
                    InvalidArgument,
                    "{} is the root package of its repository and cannot be added to the multi-package repository {}; fork it into a new folder",
                    root.original,
                    destination.display()
                );
            }
            return add_packages(
                &clone,
                &destination,
                &sources,
                version,
                self.options.custom_ssh,
            );
        }
        let parent = destination
            .parent()
            .require(Error::InvalidArgument, "destination needs a parent")?;
        // The outermost folder this creates: a failed fork leaves none of them behind.
        let created = destination
            .ancestors()
            .take_while(|p| fs::symlink_metadata(p).is_err())
            .last()
            .unwrap_or(&destination)
            .to_path_buf();
        let result = fs::create_dir_all(parent)
            .and_then(|()| fs::create_dir(&destination))
            .map_err(Error::from)
            .and_then(|()| {
                new_fork(
                    &clone,
                    &first_installed.source,
                    &destination,
                    &sources,
                    version,
                    self.options.custom_ssh,
                )
            });
        if result.is_err() {
            let _ = fs::remove_dir_all(&created);
        }
        result
    }
}

/// The forks a dependency may be rewritten to: original ID → (fork ID, release version).
fn fork_map(
    sources: &[Source],
    manifests: &[Manifest],
    version: Option<&str>,
) -> BTreeMap<String, (String, String)> {
    sources
        .iter()
        .zip(manifests)
        .map(|(s, m)| {
            (
                s.original.clone(),
                (
                    s.id.clone(),
                    version.map_or_else(|| m.version.clone(), str::to_owned),
                ),
            )
        })
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
        git(
            repo,
            &["rev-parse", "--verify", "-q", &format!("{commit}:{path}")],
            custom_ssh,
        )
        .ok()
    };
    matches!((tree(a), tree(b)), (Some(x), Some(y)) if x == y)
}

fn new_fork(
    clone: &Path,
    upstream: &str,
    destination: &Path,
    sources: &[Source],
    version: Option<&str>,
    custom_ssh: bool,
) -> Result<PackageFork> {
    let root = sources.iter().any(|s| s.path.is_empty());
    for source in sources {
        git(
            clone,
            &["cat-file", "-e", &format!("{}^{{commit}}", source.commit)],
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
    // The base is the newest installed revision; every other one must be its ancestor.
    let base = sources
        .iter()
        .map(|s| s.commit.as_str())
        .find(|candidate| {
            sources.iter().all(|s| {
                s.commit == *candidate
                    || git(
                        clone,
                        &["merge-base", "--is-ancestor", &s.commit, candidate],
                        custom_ssh,
                    )
                    .is_ok()
            })
        })
        .require(
            Error::InvalidArgument,
            "the installed revisions of these packages are on diverged histories; update them to releases of one branch first",
        )?
        .to_owned();
    copy_tree(clone, destination, true)?;
    // The workspace's keep-alive refs of a shared clone are not the author's.
    clones::drop_refs(destination, custom_ssh)?;
    let remotes = git(destination, &["remote"], custom_ssh)?;
    for remote in ["origin", "upstream"] {
        if remotes.lines().any(|r| r == remote) {
            git(destination, &["remote", "remove", remote], custom_ssh)?;
        }
    }
    git(
        destination,
        &["remote", "add", "--", "upstream", upstream],
        custom_ssh,
    )?;
    let mut branch = "fork".to_owned();
    let mut number = 2;
    while !git(destination, &["branch", "--list", &branch], custom_ssh)?.is_empty() {
        branch = format!("fork-{number}");
        number += 1;
    }
    // Attached to the new branch, so the fork commit is on it (whatever HEAD was).
    git(
        destination,
        &["checkout", "-q", "-b", &branch, &base],
        custom_ssh,
    )?;
    let mut committed: Vec<String> = Vec::new();
    if !root {
        let kept: BTreeSet<&str> = sources.iter().map(|s| s.path.as_str()).collect();
        // Siblings leave the tree (valid or not), so the fork can never release their IDs.
        let layout =
            packages::layout_at_with_ssh(destination, &base, custom_ssh)?.unwrap_or_default();
        for folder in layout.folders() {
            if kept.contains(folder) {
                continue;
            }
            git(
                destination,
                &[
                    "rm",
                    "-r",
                    "-q",
                    "--cached",
                    "--ignore-unmatch",
                    "--",
                    folder,
                ],
                custom_ssh,
            )?;
            match fs::remove_dir_all(destination.join(folder)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            committed.push(folder.to_owned());
        }
        // A package installed at an older revision starts from that revision's files.
        for source in sources.iter().filter(|s| s.commit != base) {
            if same_folder(destination, &base, &source.commit, &source.path, custom_ssh) {
                continue;
            }
            let edits = dirty(destination, std::slice::from_ref(&source.path), custom_ssh)?;
            if !edits.is_empty() {
                return Err(Error::LocalChanges {
                    id: source.original.clone(),
                    message: format!(
                        "{} is installed at an older revision than the fork starts from, and its folder {} has local edits in the clone that do not belong to that revision; update {} or commit or discard the edits first",
                        source.original, source.path, source.original
                    ),
                    items: edits
                        .into_iter()
                        .map(|path| LostWork::Modified { path })
                        .collect(),
                });
            }
            git(
                destination,
                &[
                    "rm",
                    "-r",
                    "-q",
                    "--cached",
                    "--ignore-unmatch",
                    "--",
                    &source.path,
                ],
                custom_ssh,
            )?;
            match fs::remove_dir_all(destination.join(&source.path)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            git(
                destination,
                &["checkout", &source.commit, "--", &source.path],
                custom_ssh,
            )?;
            committed.push(source.path.clone());
        }
        let config = packages::Config {
            packages: kept.iter().map(|p| (*p).to_owned()).collect(),
        };
        fs::write(
            destination.join(packages::CONFIG),
            format!("{}\n", serde_json::to_string_pretty(&config)?),
        )?;
        committed.push(packages::CONFIG.to_owned());
    }
    let mut manifests = Vec::new();
    for source in sources {
        manifests.push(package_manifest(destination, source.subdir())?);
    }
    let forks = fork_map(sources, &manifests, version);
    let mut rewrites = Vec::new();
    for (m, source) in manifests.iter_mut().zip(sources) {
        identify(m, source);
        rewrite_dependencies(m, &forks, &mut rewrites)?;
        fs::write(
            destination.join(manifest_path(&source.path)),
            manifest_json(m)?,
        )?;
        committed.push(manifest_path(&source.path));
    }
    // A new repository created by gitpm: a local demo identity only if the user has none.
    scaffold::require_identity(destination, true, custom_ssh)?;
    // Commit only the fork's own changes; keep unrelated local edits and staged files.
    let message = fork_message(sources);
    // A new repository of gitpm's: hooks of the copied clone or the user's are not run.
    let mut args = vec![
        "commit",
        "-q",
        "--no-verify",
        "--only",
        "-m",
        &message,
        "--",
    ];
    args.extend(committed.iter().map(String::as_str));
    git(destination, &args, custom_ssh)?;
    let commit = git(destination, &["rev-parse", "HEAD"], custom_ssh)?;
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

fn add_packages(
    clone: &Path,
    destination: &Path,
    sources: &[Source],
    version: Option<&str>,
    custom_ssh: bool,
) -> Result<PackageFork> {
    let repo = fs::canonicalize(destination)?;
    ensure!(
        Path::new(&git(&repo, &["rev-parse", "--show-toplevel"], custom_ssh)?) == repo,
        InvalidArgument,
        "pass the repository root"
    );
    let mut config = packages::read_config(&repo)?.unwrap_or_default();
    let existing = packages::packages_in(&repo)?.unwrap_or_default();
    for source in sources {
        ensure!(
            !existing
                .iter()
                .any(|p| p.manifest.id.eq_ignore_ascii_case(&source.id)),
            InvalidArgument,
            "package {} already exists in {}",
            source.id,
            repo.display()
        );
        ensure!(
            fs::symlink_metadata(repo.join(&source.path)).is_err(),
            InvalidArgument,
            "folder {} already exists in {}",
            source.path,
            repo.display()
        );
        for other in existing.iter().map(|p| p.path.as_str()).chain(
            sources
                .iter()
                .filter(|s| s.path != source.path)
                .map(|s| s.path.as_str()),
        ) {
            ensure!(
                !other.starts_with(&format!("{}/", source.path))
                    && !source.path.starts_with(&format!("{other}/")),
                InvalidArgument,
                "package folder {} would nest with {other}",
                source.path
            );
        }
    }
    // The manifests at the installed revisions; the folders are exported only once
    // everything is checked, straight to their place (no temporary folder in `repo`).
    let mut new_manifests: Vec<Manifest> = sources.iter().map(|s| s.base.clone()).collect();
    // Rewrite targets: the new forks, and originals already forked by this repository.
    let forks = fork_map(sources, &new_manifests, version);
    let mut aliases: BTreeMap<String, (String, String)> = BTreeMap::new();
    for p in &existing {
        for original in [
            p.manifest.fork_of.as_ref().map(|f| &f.id),
            p.manifest.replaces.as_ref().map(|r| &r.id),
        ]
        .into_iter()
        .flatten()
        {
            aliases.insert(
                original.clone(),
                (p.manifest.id.clone(), p.manifest.version.clone()),
            );
        }
    }
    let mut targets = aliases;
    targets.extend(forks.clone());
    let mut rewrites = Vec::new();
    for (m, source) in new_manifests.iter_mut().zip(sources) {
        identify(m, source);
        rewrite_dependencies(m, &targets, &mut rewrites)?;
    }
    let mut changed = Vec::new();
    for p in &existing {
        let mut m = p.manifest.clone();
        let before = rewrites.len();
        rewrite_dependencies(&mut m, &forks, &mut rewrites)?;
        if rewrites.len() > before {
            changed.push((p.path.clone(), m));
        }
    }
    let mut touched: Vec<String> = vec![packages::CONFIG.to_owned()];
    touched.extend(changed.iter().map(|(path, _)| manifest_path(path)));
    let unclean = dirty(&repo, &touched, custom_ssh)?;
    ensure!(
        unclean.is_empty(),
        InvalidArgument,
        "commit or discard the changes to {} in {} first",
        unclean.join(", "),
        repo.display()
    );
    // gitpm never writes a Git identity into the author's repository.
    scaffold::require_identity(&repo, false, custom_ssh)?;
    let originals: HashMap<String, Option<Vec<u8>>> = touched
        .iter()
        .map(|path| (path.clone(), fs::read(repo.join(path)).ok()))
        .collect();
    // A failed commit (hook, signing) must leave the index as it was, too.
    let index = PathBuf::from(git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        custom_ssh,
    )?);
    let saved_index = fs::read(&index).ok();
    let mut committed = touched.clone();
    let written = (|| {
        for (source, m) in sources.iter().zip(&new_manifests) {
            let target = repo.join(&source.path);
            committed.push(source.path.clone());
            export_folder(clone, &source.commit, &source.path, &target, custom_ssh)?;
            // The exported package must be valid as it is (dist/, dependencies).
            manifest(&target)?;
            fs::write(target.join("plugin.json"), manifest_json(m)?)?;
            if !config
                .packages
                .iter()
                .any(|pattern| packages::matches(pattern, &source.path))
            {
                config.packages.push(source.path.clone());
            }
        }
        for (path, m) in &changed {
            fs::write(repo.join(manifest_path(path)), manifest_json(m)?)?;
        }
        fs::write(
            repo.join(packages::CONFIG),
            format!("{}\n", serde_json::to_string_pretty(&config)?),
        )?;
        let mut args = vec!["add", "--"];
        args.extend(committed.iter().map(String::as_str));
        git(&repo, &args, custom_ssh)?;
        let message = fork_message(sources);
        let mut args = vec!["commit", "-q", "--only", "-m", &message, "--"];
        args.extend(committed.iter().map(String::as_str));
        git(&repo, &args, custom_ssh)
    })();
    if let Err(error) = written {
        match &saved_index {
            Some(bytes) => {
                let _ = fs::write(&index, bytes);
            }
            None => {
                let mut args = vec!["reset", "-q", "--"];
                args.extend(committed.iter().map(String::as_str));
                let _ = git(&repo, &args, custom_ssh);
            }
        }
        for source in sources {
            let _ = fs::remove_dir_all(repo.join(&source.path));
        }
        for (path, bytes) in &originals {
            match bytes {
                Some(bytes) => {
                    let _ = fs::write(repo.join(path), bytes);
                }
                None => {
                    let _ = fs::remove_file(repo.join(path));
                }
            }
        }
        return Err(match error {
            Error::Git(message) => Error::Git(format!(
                "adding the forked packages to {} failed; the repository was restored: {message}",
                repo.display()
            )),
            error => error,
        });
    }
    Ok(PackageFork {
        path: repo.clone(),
        created: false,
        commit: git(&repo, &["rev-parse", "HEAD"], custom_ssh)?,
        packages: forked(sources, &new_manifests),
        rewrites,
    })
}

/// Install order of packages given as (ID, dependency IDs): indices with dependencies
/// on other listed packages first. A dependency cycle between them is an error: such
/// packages cannot be installed one after another.
fn install_order(packages: &[(&str, BTreeSet<String>)]) -> Result<Vec<usize>> {
    fn visit(
        index: usize,
        packages: &[(&str, BTreeSet<String>)],
        path: &mut Vec<usize>,
        done: &mut BTreeSet<usize>,
        order: &mut Vec<usize>,
    ) -> Result<()> {
        if done.contains(&index) {
            return Ok(());
        }
        if let Some(start) = path.iter().position(|i| *i == index) {
            let cycle: Vec<&str> = path[start..]
                .iter()
                .chain([&index])
                .map(|i| packages[*i].0)
                .collect();
            fail!(
                DependencyConflict,
                "dependency cycle between the forked packages: {}; they cannot be installed one after another",
                cycle.join(" → ")
            );
        }
        path.push(index);
        for dependency in &packages[index].1 {
            if let Some(other) = packages.iter().position(|(id, _)| id == dependency) {
                visit(other, packages, path, done, order)?;
            }
        }
        path.pop();
        done.insert(index);
        order.push(index);
        Ok(())
    }
    let (mut path, mut done, mut order) = (Vec::new(), BTreeSet::new(), Vec::new());
    for index in 0..packages.len() {
        visit(index, packages, &mut path, &mut done, &mut order)?;
    }
    Ok(order)
}

/// Release the forked packages: set `version` (when given) in each manifest, commit the
/// changes in their folders once (the whole repository for a root package, like
/// `commit`), then tag each `ID@VERSION`. On failure: the tags created so far and the
/// error.
fn release_packages(
    fork: &PackageFork,
    version: Option<&str>,
    custom_ssh: bool,
) -> std::result::Result<(), (BTreeMap<String, String>, Error)> {
    let repo = &fork.path;
    let mut tagged = BTreeMap::new();
    let fail = |tagged: &BTreeMap<String, String>, error| Err((tagged.clone(), error));
    let mut tags = Vec::new();
    for package in &fork.packages {
        let tag = identity::release_tag(&package.id, version.unwrap_or(&package.version));
        match git(repo, &["tag", "--list", "--", &tag], custom_ssh) {
            Ok(found) if found.is_empty() => tags.push((package.id.clone(), tag)),
            Ok(_) => {
                return fail(
                    &tagged,
                    Error::version_conflict(format!("tag already exists: {tag}")),
                );
            }
            Err(error) => return fail(&tagged, error),
        }
    }
    let folders: Vec<&str> = fork.packages.iter().map(|p| pathspec(&p.path)).collect();
    let committed = (|| {
        let mut saved = Vec::new();
        if let Some(version) = version {
            for package in fork.packages.iter().filter(|p| p.version != version) {
                let path = repo.join(manifest_path(&package.path));
                let original = fs::read(&path)?;
                let subdir = (!package.path.is_empty()).then_some(package.path.as_str());
                let mut m = package_manifest(repo, subdir)?;
                m.version = version.to_owned();
                fs::write(&path, manifest_json(&m)?)?;
                saved.push((path, original));
            }
        }
        let result = (|| {
            let mut args = vec!["add", "--all", "--"];
            args.extend(&folders);
            git(repo, &args, custom_ssh)?;
            let mut args = vec!["status", "--porcelain", "--"];
            args.extend(&folders);
            if git(repo, &args, custom_ssh)?.is_empty() {
                return Ok(());
            }
            let names: Vec<String> = fork
                .packages
                .iter()
                .map(|p| format!("{} {}", p.id, version.unwrap_or(&p.version)))
                .collect();
            let message = format!("Release {}", names.join(", "));
            let mut args = vec!["commit", "-q", "--only", "-m", &message, "--"];
            args.extend(&folders);
            git(repo, &args, custom_ssh).map(drop)
        })();
        if let Err(error) = result {
            for (path, bytes) in saved {
                let _ = fs::write(path, bytes);
            }
            return Err(Error::Git(format!(
                "release commit failed; the manifests were restored: {error}"
            )));
        }
        Ok(())
    })();
    if let Err(error) = committed {
        return fail(&tagged, error);
    }
    for (id, tag) in tags {
        if let Err(error) = git(repo, &["tag", "--", &tag], custom_ssh) {
            return fail(&tagged, error);
        }
        tagged.insert(id, tag);
    }
    Ok(())
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

    #[test]
    fn dirty_sees_untracked_files_whatever_the_status_config_says() {
        use crate::test_support::{TempDir, git as fixture_git};
        let dir = TempDir::new("dirty");
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join("packages/a")).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("packages/a/plugin.json"), "{}").unwrap();
        fixture_git(&repo, &["add", "--all"]);
        fixture_git(&repo, &["commit", "-q", "-m", "first"]);
        // A setting of the user's that hides untracked files from a plain status.
        fixture_git(&repo, &["config", "status.showUntrackedFiles", "no"]);
        fs::write(repo.join(packages::CONFIG), "{}").unwrap();
        fs::write(repo.join("packages/a/new.js"), "x").unwrap();
        fs::write(repo.join("packages/a/plugin.json"), "{ }").unwrap();
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain"]),
            "M packages/a/plugin.json"
        );
        let paths = [packages::CONFIG.to_owned(), "packages/a".to_owned()];
        let mut found = dirty(&repo, &paths, false).unwrap();
        found.sort();
        assert_eq!(
            found,
            [
                packages::CONFIG,
                "packages/a/new.js",
                "packages/a/plugin.json"
            ]
        );
        assert!(
            dirty(&repo, &["other".to_owned()], false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn install_order_puts_dependencies_first_and_names_a_cycle() {
        let set = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<_>>();
        let packages = [
            ("me/panel", set(&["me/theme", "x/other"])),
            ("me/theme", set(&["me/base"])),
            ("me/base", set(&[])),
        ];
        assert_eq!(install_order(&packages).unwrap(), [2, 1, 0]);
        let cyclic = [
            ("me/a", set(&["me/b"])),
            ("me/b", set(&["me/c"])),
            ("me/c", set(&["me/a"])),
        ];
        let error = install_order(&cyclic).unwrap_err();
        assert_eq!(error.kind(), "dependency_conflict");
        assert!(
            error.to_string().contains("me/a → me/b → me/c → me/a"),
            "{error}"
        );
    }
}
