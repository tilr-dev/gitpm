//! Package dependencies: version requirements (Cargo syntax), resolution with PubGrub
//! over installed packages and the catalog, and the dependency view of a workspace.
//!
//! Installed packages are fixed at their installed version: resolution only adds
//! missing packages and never moves an installed one. A replaced original is seen by
//! the solver as its replacement: the fork's declared `replaces.version` and the
//! fork's own dependencies.
use crate::error::{Error, Result, ensure};
use crate::{Installed, Manifest, State, catalog};
use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyConstraints, DependencyProvider,
    PackageResolutionStatistics, PubGrubError, Reporter,
};
use semver::{Comparator, Op, Version, VersionReq};
use serde::Serialize;
use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub type Range = pubgrub::Ranges<Version>;

/// `MAJOR.MINOR.PATCH` as a semver version.
pub fn version(text: &str) -> Result<Version> {
    let (major, minor, patch) = catalog::version_key(text)?;
    Ok(Version::new(major, minor, patch))
}

/// Parse a Cargo-style requirement (`1.2`, `^1.2.3`, `~1.2`, `>=1.3, <2`, `=1.0.0`,
/// `1.*`, `*`) into the set of release versions it allows. Pre-releases are not used.
pub fn requirement(text: &str) -> Result<Range> {
    let invalid = |reason: String| {
        Error::InvalidManifest(format!("invalid version requirement {text:?}: {reason}"))
    };
    let parsed = VersionReq::parse(text).map_err(|e| invalid(e.to_string()))?;
    let mut range = Range::full();
    for comparator in &parsed.comparators {
        if !comparator.pre.is_empty() {
            return Err(invalid("pre-release versions are not supported".into()));
        }
        range = range.intersection(
            &comparator_range(comparator).ok_or_else(|| invalid("unsupported operator".into()))?,
        );
    }
    Ok(range)
}

fn comparator_range(c: &Comparator) -> Option<Range> {
    let v = Version::new;
    let lower = v(c.major, c.minor.unwrap_or(0), c.patch.unwrap_or(0));
    // The first version after everything the comparator spells out.
    let after = match (c.minor, c.patch) {
        (None, _) => v(c.major + 1, 0, 0),
        (Some(minor), None) => v(c.major, minor + 1, 0),
        (Some(minor), Some(patch)) => v(c.major, minor, patch + 1),
    };
    Some(match c.op {
        Op::Exact | Op::Wildcard => Range::between(lower, after),
        Op::Greater => Range::higher_than(after),
        Op::GreaterEq => Range::higher_than(lower),
        Op::Less => Range::strictly_lower_than(lower),
        Op::LessEq => Range::strictly_lower_than(after),
        Op::Tilde => match c.minor {
            None => Range::between(lower, v(c.major + 1, 0, 0)),
            Some(minor) => Range::between(lower, v(c.major, minor + 1, 0)),
        },
        Op::Caret => {
            let upper = match (c.major, c.minor, c.patch) {
                (0, None, _) => v(1, 0, 0),
                (0, Some(0), None) => v(0, 1, 0),
                (0, Some(0), Some(patch)) => v(0, 0, patch + 1),
                (0, Some(minor), _) => v(0, minor + 1, 0),
                (major, _, _) => v(major + 1, 0, 0),
            };
            Range::between(lower, upper)
        }
        _ => return None,
    })
}

/// Does `version` satisfy `requirement`? Unparsable input never matches.
pub fn matches(requirement_text: &str, version_text: &str) -> bool {
    match (requirement(requirement_text), version(version_text)) {
        (Ok(range), Ok(version)) => range.contains(&version),
        _ => false,
    }
}

/// Validate a manifest's `dependencies` (IDs, requirements, no self-dependency).
pub(crate) fn validate(m: &Manifest) -> Result<()> {
    for (id, text) in &m.dependencies {
        crate::valid_id(id)?;
        ensure!(
            *id != m.id,
            InvalidManifest,
            "package {} depends on itself",
            m.id
        );
        requirement(text)?;
    }
    Ok(())
}

// ---- workspace view -------------------------------------------------------------------

/// How one dependency of an installed package is provided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DependencyStatus {
    /// The ID the package depends on.
    pub id: String,
    pub requirement: String,
    /// The installed package that provides it (the replacement fork when replaced).
    pub provider: Option<String>,
    /// Version the provider offers for this ID (a fork's declared `replaces.version`).
    pub version: Option<String>,
    pub satisfied: bool,
    /// Why it is not satisfied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// The installed package providing `id` and the version it offers for it.
pub(crate) fn provider<'a>(state: &'a State, id: &str) -> Option<(&'a str, &'a Installed, String)> {
    match state.replacements.get(id) {
        Some(fork) => {
            let (fork, installed) = state.packages.get_key_value(fork)?;
            let declared = installed.replaces.as_ref().filter(|r| r.id == id)?;
            Some((fork.as_str(), installed, declared.version.clone()))
        }
        None => {
            let (id, installed) = state.packages.get_key_value(id)?;
            Some((id.as_str(), installed, installed.current.version.clone()))
        }
    }
}

/// [`DependencyStatus::problem`] of a dependency no installed package provides (and
/// that no replacement redirects).
pub(crate) const NOT_INSTALLED: &str = "not installed";

pub(crate) fn status(state: &State, plugin: &Installed) -> Vec<DependencyStatus> {
    plugin
        .dependencies
        .iter()
        .map(|(id, text)| {
            let mut row = DependencyStatus {
                id: id.clone(),
                requirement: text.clone(),
                provider: None,
                version: None,
                satisfied: false,
                problem: None,
            };
            match provider(state, id) {
                None => {
                    row.problem = Some(match state.replacements.get(id) {
                        Some(fork) => format!("replacement {fork} is not installed"),
                        None => NOT_INSTALLED.into(),
                    });
                }
                Some((provider, installed, offered)) => {
                    row.provider = Some(provider.into());
                    row.version = Some(offered.clone());
                    row.problem = if !installed.enabled {
                        Some(format!("{provider} is disabled"))
                    } else if !matches(text, &offered) {
                        Some(format!("version {offered} does not satisfy {text}"))
                    } else {
                        None
                    };
                    row.satisfied = row.problem.is_none();
                }
            }
            row
        })
        .collect()
}

/// Installed packages (other than `provider` itself) whose dependencies are provided by
/// `provider`, sorted.
pub(crate) fn dependents(state: &State, provider_id: &str) -> Vec<String> {
    state
        .packages
        .iter()
        .filter(|(id, plugin)| {
            id.as_str() != provider_id
                && plugin
                    .dependencies
                    .keys()
                    .any(|dep| provider(state, dep).is_some_and(|(p, _, _)| p == provider_id))
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Refuse with [`Error::DependencyConflict`] when a direct dependency of `id` (about to
/// be active) is not satisfied by an active package: disabled ("DEP is disabled; enable
/// it"), missing, replaced away or of a non-matching version. Dependencies of the
/// providers are theirs (a broken provider is reported, not enforced). `forceable` adds
/// that forcing does it anyway.
pub(crate) fn refuse_inactive_dependencies(
    state: &State,
    id: &str,
    action: &str,
    forceable: bool,
) -> Result<()> {
    let Some(package) = state.packages.get(id) else {
        return Ok(());
    };
    let problems: Vec<String> = status(state, package)
        .into_iter()
        .filter(|row| !row.satisfied)
        .map(|row| match &row.provider {
            Some(provider) if state.packages.get(provider).is_some_and(|p| !p.enabled) => {
                format!("{provider} is disabled; enable it")
            }
            _ => format!(
                "{} {}: {}",
                row.id,
                row.requirement,
                row.problem.as_deref().unwrap_or("not satisfied")
            ),
        })
        .collect();
    if problems.is_empty() {
        return Ok(());
    }
    Err(Error::DependencyConflict(format!(
        "cannot {action} {id}: {}{}",
        problems.join("; "),
        if forceable {
            "; force does it anyway"
        } else {
            ""
        }
    )))
}

// ---- resolution -----------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Package {
    Workspace,
    Id(String),
}
impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Package::Workspace => f.write_str("the workspace"),
            Package::Id(id) => f.write_str(id),
        }
    }
}

/// A package whose version is decided already (installed, being installed or updated).
#[derive(Clone, Debug)]
pub(crate) struct Fixed {
    pub version: Version,
    pub dependencies: BTreeMap<String, String>,
    /// Shown in explanations, e.g. "replaced by mine/base".
    pub note: Option<String>,
}

/// Fixed packages for a workspace: installed packages at their versions, replaced
/// originals as their replacement.
pub(crate) fn fixed_from_state(state: &State) -> Result<BTreeMap<String, Fixed>> {
    let mut fixed = BTreeMap::new();
    for (id, plugin) in &state.packages {
        fixed.insert(
            id.clone(),
            Fixed {
                version: version(&plugin.current.version)?,
                dependencies: plugin.dependencies.clone(),
                note: Some(format!("installed at {}", plugin.current.version)),
            },
        );
    }
    for (original, fork) in &state.replacements {
        if let Some((_, installed, offered)) = provider(state, original) {
            fixed.insert(
                original.clone(),
                Fixed {
                    version: version(&offered)?,
                    dependencies: installed.dependencies.clone(),
                    note: Some(format!("replaced by {fork}")),
                },
            );
        }
    }
    Ok(fixed)
}

struct Provider<'a> {
    fixed: &'a BTreeMap<String, Fixed>,
    roots: &'a BTreeMap<String, Version>,
    catalog: Option<&'a str>,
    versions: RefCell<BTreeMap<String, Vec<Version>>>,
}

impl Provider<'_> {
    fn catalog_versions(&self, id: &str) -> Result<Vec<Version>> {
        if let Some(cached) = self.versions.borrow().get(id) {
            return Ok(cached.clone());
        }
        let mut found = match self.catalog {
            Some(location) => catalog::versions(location, id)?
                .iter()
                .filter_map(|v| version(v).ok())
                .collect(),
            None => Vec::new(),
        };
        found.sort_by(|a, b| b.cmp(a));
        self.versions.borrow_mut().insert(id.into(), found.clone());
        Ok(found)
    }
    fn constraints(
        &self,
        dependencies: &BTreeMap<String, String>,
    ) -> Result<DependencyConstraints<Package, Range>> {
        dependencies
            .iter()
            .map(|(id, text)| Ok((Package::Id(id.clone()), requirement(text)?)))
            .collect()
    }
}

impl DependencyProvider for Provider<'_> {
    type P = Package;
    type V = Version;
    type VS = Range;
    type M = String;
    type Err = Error;
    type Priority = (bool, Reverse<usize>);

    fn prioritize(
        &self,
        package: &Package,
        range: &Range,
        _: &PackageResolutionStatistics,
    ) -> Self::Priority {
        match package {
            Package::Workspace => (true, Reverse(0)),
            Package::Id(id) if self.fixed.contains_key(id) => (true, Reverse(1)),
            Package::Id(id) => {
                let count = self
                    .versions
                    .borrow()
                    .get(id)
                    .map_or(0, |all| all.iter().filter(|v| range.contains(v)).count());
                (false, Reverse(count))
            }
        }
    }

    fn choose_version(&self, package: &Package, range: &Range) -> Result<Option<Version>> {
        Ok(match package {
            Package::Workspace => Some(Version::new(0, 0, 0)),
            Package::Id(id) => match self.fixed.get(id) {
                Some(fixed) => range
                    .contains(&fixed.version)
                    .then(|| fixed.version.clone()),
                None => self
                    .catalog_versions(id)?
                    .into_iter()
                    .find(|v| range.contains(v)),
            },
        })
    }

    fn get_dependencies(
        &self,
        package: &Package,
        version: &Version,
    ) -> Result<Dependencies<Package, Range, String>> {
        Ok(Dependencies::Available(match package {
            Package::Workspace => self
                .roots
                .iter()
                .map(|(id, v)| (Package::Id(id.clone()), Range::singleton(v.clone())))
                .collect(),
            Package::Id(id) => match self.fixed.get(id) {
                Some(fixed) => self.constraints(&fixed.dependencies)?,
                None => {
                    let location = self
                        .catalog
                        .ok_or_else(|| Error::Catalog("no catalog configured".into()))?;
                    let release = catalog::lookup(location, id, Some(&version.to_string()))?;
                    self.constraints(&release.manifest.dependencies)?
                }
            },
        }))
    }
}

/// Resolve `roots` (each required at exactly its version) on top of `fixed`. Returns
/// the packages that are neither fixed nor roots, i.e. what must be fetched from the
/// catalog, with their chosen versions.
pub(crate) fn solve(
    fixed: &BTreeMap<String, Fixed>,
    roots: &BTreeMap<String, Version>,
    catalog: Option<&str>,
) -> Result<BTreeMap<String, Version>> {
    let provider = Provider {
        fixed,
        roots,
        catalog,
        versions: RefCell::new(BTreeMap::new()),
    };
    match pubgrub::resolve(&provider, Package::Workspace, Version::new(0, 0, 0)) {
        Ok(solution) => Ok(solution
            .into_iter()
            .filter_map(|(package, version)| match package {
                Package::Id(id) if !fixed.contains_key(&id) => Some((id, version)),
                _ => None,
            })
            .collect()),
        Err(PubGrubError::NoSolution(mut tree)) => {
            tree.collapse_no_versions();
            let mut message = format!(
                "dependencies cannot be satisfied:\n{}",
                DefaultStringReporter::report(&tree)
            );
            let report = message.clone();
            let notes: BTreeSet<_> = fixed
                .iter()
                .filter(|(id, _)| report.contains(id.as_str()))
                .filter_map(|(id, f)| f.note.as_ref().map(|n| format!("{id} is {n}")))
                .collect();
            for note in notes {
                message.push_str(&format!("\nnote: {note}"));
            }
            if catalog.is_none() {
                message.push_str("\nnote: no catalog configured; only installed packages can satisfy dependencies");
            }
            Err(Error::DependencyConflict(message))
        }
        Err(PubGrubError::ErrorRetrievingDependencies { source, .. })
        | Err(PubGrubError::ErrorChoosingVersion { source, .. }) => Err(source),
        Err(other) => Err(Error::DependencyConflict(other.to_string())),
    }
}

/// Roots for a workspace check: every installed consumer (package with dependencies)
/// whose dependencies are all satisfied now. Already broken ones (after a forced
/// removal) are reported by `status` instead of blocking every later operation;
/// replaced originals are not used. Packages without dependencies constrain nothing
/// and stay out, so explanations name the consumers.
pub(crate) fn healthy_roots(state: &State) -> Result<BTreeMap<String, Version>> {
    let mut roots = BTreeMap::new();
    for (id, plugin) in &state.packages {
        if state.replacements.contains_key(id) || plugin.dependencies.is_empty() {
            continue;
        }
        if status(state, plugin).iter().all(|s| s.satisfied) {
            roots.insert(id.clone(), version(&plugin.current.version)?);
        }
    }
    Ok(roots)
}

/// The direct dependencies of the installed package `id` that make it a broken consumer
/// for `update all`: unsatisfied now (see [`status`]). A replaced original consumes
/// nothing; an inactive package takes a disabled provider of a matching version as
/// given (as `download` and the update of a disabled package do).
pub(crate) fn broken_dependencies(state: &State, id: &str) -> Vec<DependencyStatus> {
    let Some(package) = state.packages.get(id) else {
        return Vec::new();
    };
    if state.replacements.contains_key(id) {
        return Vec::new();
    }
    let active = state.is_active(id);
    status(state, package)
        .into_iter()
        .filter(|row| {
            !row.satisfied
                && (active
                    || row.provider.is_none()
                    || !row
                        .version
                        .as_deref()
                        .is_some_and(|v| matches(&row.requirement, v)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allows(req: &str, yes: &[&str], no: &[&str]) {
        let range = requirement(req).unwrap();
        for v in yes {
            assert!(range.contains(&version(v).unwrap()), "{req} must allow {v}");
        }
        for v in no {
            assert!(
                !range.contains(&version(v).unwrap()),
                "{req} must reject {v}"
            );
        }
    }

    #[test]
    fn requirements_follow_cargo_semantics() {
        allows("1.2.3", &["1.2.3", "1.9.0"], &["1.2.2", "2.0.0"]);
        allows("^1.2", &["1.2.0", "1.99.0"], &["1.1.9", "2.0.0"]);
        allows("^0.2.3", &["0.2.3", "0.2.9"], &["0.3.0", "0.2.2"]);
        allows("^0.0.3", &["0.0.3"], &["0.0.4", "0.0.2"]);
        allows("^0", &["0.0.0", "0.9.9"], &["1.0.0"]);
        allows("~1.2.3", &["1.2.3", "1.2.9"], &["1.3.0", "1.2.2"]);
        allows("~1", &["1.0.0", "1.9.0"], &["2.0.0"]);
        allows("=1.0.0", &["1.0.0"], &["1.0.1", "0.9.9"]);
        allows("=1.2", &["1.2.0", "1.2.7"], &["1.3.0"]);
        allows(">=1.3", &["1.3.0", "9.0.0"], &["1.2.9"]);
        allows(">1.2.3", &["1.2.4"], &["1.2.3"]);
        allows(">1.2", &["1.3.0"], &["1.2.9"]);
        allows("<2", &["1.9.9"], &["2.0.0"]);
        allows("<=1.2", &["1.2.9"], &["1.3.0"]);
        allows(">=1.0, <1.5", &["1.4.9"], &["1.5.0", "0.9.0"]);
        allows("1.*", &["1.0.0", "1.9.0"], &["2.0.0"]);
        allows("*", &["0.0.1", "99.0.0"], &[]);
        for bad in ["", "banana", "^1.0.0-beta", ">=1.0.0-rc.1", "1.0.0.0"] {
            assert_eq!(
                requirement(bad).unwrap_err().kind(),
                "invalid_manifest",
                "{bad:?}"
            );
        }
        assert!(matches("^1.0", "1.4.0"));
        assert!(!matches("^1.0", "2.0.0"));
        assert!(!matches("^1.0", "1.0"));
    }

    fn fixed(version_text: &str, deps: &[(&str, &str)]) -> Fixed {
        Fixed {
            version: version(version_text).unwrap(),
            dependencies: deps
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            note: None,
        }
    }

    #[test]
    fn solver_explains_conflicts_naming_both_consumers() {
        let pinned = BTreeMap::from([
            ("lib/core".to_string(), fixed("1.1.0", &[])),
            ("app/a".to_string(), fixed("1.0.0", &[("lib/core", "^1.0")])),
            ("app/b".to_string(), fixed("1.0.0", &[("lib/core", "^2.0")])),
        ]);
        let roots = BTreeMap::from([
            ("app/a".to_string(), version("1.0.0").unwrap()),
            ("app/b".to_string(), version("1.0.0").unwrap()),
        ]);
        let error = solve(&pinned, &roots, None).unwrap_err();
        assert_eq!(error.kind(), "dependency_conflict");
        let text = error.to_string();
        assert!(text.contains("app/a") && text.contains("app/b"), "{text}");
        assert!(text.contains("lib/core"), "{text}");
    }

    #[test]
    fn solver_accepts_cycles_and_reports_missing_packages() {
        let pinned = BTreeMap::from([
            ("x/a".to_string(), fixed("1.0.0", &[("x/b", "^1")])),
            ("x/b".to_string(), fixed("1.0.0", &[("x/a", "^1")])),
        ]);
        let roots = BTreeMap::from([("x/a".to_string(), version("1.0.0").unwrap())]);
        assert!(solve(&pinned, &roots, None).unwrap().is_empty());
        let pinned = BTreeMap::from([("x/a".to_string(), fixed("1.0.0", &[("x/missing", "^1")]))]);
        let error = solve(&pinned, &roots, None).unwrap_err();
        assert!(error.to_string().contains("x/missing"), "{error}");
        assert!(
            error.to_string().contains("no catalog configured"),
            "{error}"
        );
    }
}
