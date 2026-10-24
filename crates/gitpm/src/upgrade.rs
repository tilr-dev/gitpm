//! Joint upgrade resolution. Installed versions are lower bounds, not pins.
use crate::{
    Error, Result, State, catalog,
    deps::{self, Package, Range},
};
use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyConstraints, DependencyProvider,
    PackageResolutionStatistics, PubGrubError, Reporter,
};
use semver::Version;
use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

pub(crate) type Candidates = BTreeMap<String, BTreeMap<Version, catalog::Release>>;
struct Provider<'a> {
    state: &'a State,
    /// Broken consumers and their unsatisfied dependencies, left out of the solve: such
    /// a dependency is never installed for them.
    broken: &'a BTreeMap<String, BTreeSet<String>>,
    /// New packages (not installed yet) solved as active: an active package pulls them.
    active_new: &'a BTreeSet<String>,
    catalog: Option<&'a str>,
    candidates: RefCell<Candidates>,
}
impl Provider<'_> {
    fn candidates(&self, id: &str) -> Result<BTreeMap<Version, catalog::Release>> {
        if let Some(found) = self.candidates.borrow().get(id) {
            return Ok(found.clone());
        }
        let mut found = BTreeMap::new();
        if let Some(location) = self.catalog {
            for version in catalog::versions(location, id)? {
                let release = catalog::lookup(location, id, Some(&version))?;
                catalog::validate_release(&release, id, &version)?;
                found.insert(deps::version(&version)?, release);
            }
        }
        self.candidates
            .borrow_mut()
            .insert(id.into(), found.clone());
        Ok(found)
    }
    fn constraints(
        &self,
        consumer: &str,
        dependencies: &BTreeMap<String, String>,
    ) -> Result<DependencyConstraints<Package, Range>> {
        let ignored = self.broken.get(consumer);
        // Only active packages satisfy an active consumer; an inactive one (and a new
        // package only inactive consumers pull) takes disabled providers as given.
        let active = if self.state.packages.contains_key(consumer) {
            self.state.is_active(consumer)
        } else {
            self.active_new.contains(consumer)
        };
        let mut result = BTreeMap::new();
        for (id, text) in dependencies {
            if ignored.is_some_and(|ignored| ignored.contains(id)) {
                continue;
            }
            // A refusal from the consumer's own manifest names the consumer.
            let requirement =
                deps::requirement(text).map_err(|error| error.with_package(consumer))?;
            let provider = self.state.replacements.get(id).unwrap_or(id);
            let mut allowed = if provider != id {
                let mut allowed = Range::empty();
                for (version, release) in self.candidates(provider)? {
                    if let Some(replaces) = &release.manifest.replaces
                        && replaces.id == *id
                        && requirement.contains(&deps::version(&replaces.version)?)
                    {
                        allowed = allowed.union(&Range::singleton(version));
                    }
                }
                allowed
            } else {
                requirement
            };
            if active
                && self
                    .state
                    .packages
                    .get(provider)
                    .is_some_and(|p| !p.enabled)
            {
                allowed = Range::empty();
            }
            let key = Package::Id(provider.clone());
            if let Some(previous) = result.get(&key) {
                allowed = allowed.intersection(previous);
            }
            result.insert(key, allowed);
        }
        Ok(result.into_iter().collect())
    }
}
impl DependencyProvider for Provider<'_> {
    type P = Package;
    type V = Version;
    type VS = Range;
    type M = String;
    type Err = Error;
    type Priority = (bool, Reverse<String>);
    fn prioritize(
        &self,
        package: &Package,
        _: &Range,
        _: &PackageResolutionStatistics,
    ) -> Self::Priority {
        match package {
            Package::Workspace => (true, Reverse(String::new())),
            Package::Id(id) => (self.state.packages.contains_key(id), Reverse(id.clone())),
        }
    }
    fn choose_version(&self, package: &Package, range: &Range) -> Result<Option<Version>> {
        Ok(match package {
            Package::Workspace => Some(Version::new(0, 0, 0)),
            Package::Id(id) => self
                .candidates(id)?
                .into_keys()
                .rev()
                .find(|v| range.contains(v)),
        })
    }
    fn get_dependencies(
        &self,
        package: &Package,
        version: &Version,
    ) -> Result<Dependencies<Package, Range, String>> {
        let constraints = match package {
            Package::Workspace => self
                .state
                .packages
                .iter()
                .map(|(id, p)| {
                    Ok((
                        Package::Id(id.clone()),
                        if p.link().is_some() {
                            Range::singleton(deps::version(&p.current.version)?)
                        } else {
                            Range::higher_than(deps::version(&p.current.version)?)
                        },
                    ))
                })
                .collect::<Result<_>>()?,
            Package::Id(id) => {
                let release = self.candidates(id)?.remove(version).ok_or_else(|| {
                    Error::DependencyConflict(format!("missing candidate {id}@{version}"))
                })?;
                // A replaced original is retained but is not an active consumer.
                if self.state.replacements.contains_key(id) {
                    DependencyConstraints::default()
                } else {
                    self.constraints(id, &release.manifest.dependencies)?
                }
            }
        };
        Ok(Dependencies::Available(constraints))
    }
}

/// The joint solution and the new packages in it that an active package pulls
/// (directly or through other new packages); only those are installed enabled.
pub(crate) struct Solution {
    pub selected: BTreeMap<String, catalog::Release>,
    pub active_new: BTreeSet<String>,
}

/// Solve, first taking every new package as inactive; a new package the solution shows
/// pulled by an active one is solved again as active (disabled providers satisfy it no
/// more), until the active new packages are the ones solved as active.
pub(crate) fn solve(
    state: &State,
    broken: &BTreeMap<String, BTreeSet<String>>,
    mut candidates: Candidates,
    catalog: Option<&str>,
) -> Result<Solution> {
    let mut assumed = BTreeSet::new();
    loop {
        let (selected, loaded) = solve_once(state, broken, &assumed, candidates, catalog)?;
        candidates = loaded;
        let active_new = active_new(state, broken, &selected);
        if active_new.is_subset(&assumed) {
            return Ok(Solution {
                selected,
                active_new,
            });
        }
        assumed.extend(active_new);
    }
}

/// New packages of `selected` reachable from an active installed package over the
/// dependencies of the selected releases (broken consumers' unsatisfied ones left out).
fn active_new(
    state: &State,
    broken: &BTreeMap<String, BTreeSet<String>>,
    selected: &BTreeMap<String, catalog::Release>,
) -> BTreeSet<String> {
    let mut active = BTreeSet::new();
    let mut queue: Vec<&str> = selected
        .keys()
        .filter(|id| state.is_active(id))
        .map(String::as_str)
        .collect();
    while let Some(id) = queue.pop() {
        let Some(release) = selected.get(id) else {
            continue;
        };
        let ignored = broken.get(id);
        for dependency in release.manifest.dependencies.keys() {
            if ignored.is_some_and(|ignored| ignored.contains(dependency)) {
                continue;
            }
            let provider = state.replacements.get(dependency).unwrap_or(dependency);
            if !state.packages.contains_key(provider) && active.insert(provider.clone()) {
                queue.push(provider);
            }
        }
    }
    active
}

fn solve_once(
    state: &State,
    broken: &BTreeMap<String, BTreeSet<String>>,
    active_new: &BTreeSet<String>,
    candidates: Candidates,
    catalog: Option<&str>,
) -> Result<(BTreeMap<String, catalog::Release>, Candidates)> {
    let provider = Provider {
        state,
        broken,
        active_new,
        catalog,
        candidates: RefCell::new(candidates),
    };
    let selected = match pubgrub::resolve(&provider, Package::Workspace, Version::new(0, 0, 0)) {
        Ok(solution) => solution
            .into_iter()
            .filter_map(|(p, v)| match p {
                Package::Id(id) => Some(provider.candidates(&id).and_then(|mut candidates| {
                    candidates.remove(&v).map(|r| (id, r)).ok_or_else(|| {
                        Error::DependencyConflict("selected candidate disappeared".into())
                    })
                })),
                Package::Workspace => None,
            })
            .collect::<Result<BTreeMap<_, _>>>()?,
        Err(PubGrubError::NoSolution(mut tree)) => {
            tree.collapse_no_versions();
            return Err(Error::DependencyConflict(format!(
                "no compatible upgrade without removals or downgrades:\n{}",
                DefaultStringReporter::report(&tree)
            )));
        }
        Err(PubGrubError::ErrorRetrievingDependencies { source, .. })
        | Err(PubGrubError::ErrorChoosingVersion { source, .. }) => return Err(source),
        Err(other) => return Err(Error::DependencyConflict(other.to_string())),
    };
    Ok((selected, provider.candidates.into_inner()))
}
