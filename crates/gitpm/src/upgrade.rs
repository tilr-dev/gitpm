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
use std::{cell::RefCell, cmp::Reverse, collections::BTreeMap};

pub(crate) type Candidates = BTreeMap<String, BTreeMap<Version, catalog::Release>>;
struct Provider<'a> {
    state: &'a State,
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
        dependencies: &BTreeMap<String, String>,
    ) -> Result<DependencyConstraints<Package, Range>> {
        let mut result = BTreeMap::new();
        for (id, text) in dependencies {
            let requirement = deps::requirement(text)?;
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
            if self.state.plugins.get(provider).is_some_and(|p| !p.enabled) {
                allowed = Range::empty();
            }
            let key = Package::Plugin(provider.clone());
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
            Package::Plugin(id) => (self.state.plugins.contains_key(id), Reverse(id.clone())),
        }
    }
    fn choose_version(&self, package: &Package, range: &Range) -> Result<Option<Version>> {
        Ok(match package {
            Package::Workspace => Some(Version::new(0, 0, 0)),
            Package::Plugin(id) => self
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
                .plugins
                .iter()
                .map(|(id, p)| {
                    Ok((
                        Package::Plugin(id.clone()),
                        Range::higher_than(deps::version(&p.current.version)?),
                    ))
                })
                .collect::<Result<_>>()?,
            Package::Plugin(id) => {
                let release = self.candidates(id)?.remove(version).ok_or_else(|| {
                    Error::DependencyConflict(format!("missing candidate {id}@{version}"))
                })?;
                // A replaced original is retained but is not an active consumer.
                if self.state.replacements.contains_key(id) {
                    DependencyConstraints::default()
                } else {
                    self.constraints(&release.manifest.dependencies)?
                }
            }
        };
        Ok(Dependencies::Available(constraints))
    }
}

pub(crate) fn solve(
    state: &State,
    candidates: Candidates,
    catalog: Option<&str>,
) -> Result<BTreeMap<String, catalog::Release>> {
    let provider = Provider {
        state,
        catalog,
        candidates: RefCell::new(candidates),
    };
    match pubgrub::resolve(&provider, Package::Workspace, Version::new(0, 0, 0)) {
        Ok(solution) => solution
            .into_iter()
            .filter_map(|(p, v)| match p {
                Package::Plugin(id) => Some(provider.candidates(&id).and_then(|mut candidates| {
                    candidates.remove(&v).map(|r| (id, r)).ok_or_else(|| {
                        Error::DependencyConflict("selected candidate disappeared".into())
                    })
                })),
                Package::Workspace => None,
            })
            .collect(),
        Err(PubGrubError::NoSolution(mut tree)) => {
            tree.collapse_no_versions();
            Err(Error::DependencyConflict(format!(
                "no compatible upgrade without removals or downgrades:\n{}",
                DefaultStringReporter::report(&tree)
            )))
        }
        Err(PubGrubError::ErrorRetrievingDependencies { source, .. })
        | Err(PubGrubError::ErrorChoosingVersion { source, .. }) => Err(source),
        Err(other) => Err(Error::DependencyConflict(other.to_string())),
    }
}
