//! Package dependencies: version requirements (npm syntax), resolution with PubGrub
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
use semver::{Prerelease, Version};
use serde::Serialize;
use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Bound,
};

/// A set of versions as npm ranges see them: the releases (no pre-release) it
/// contains, and separately the pre-releases it contains. Both lanes are kept in a
/// canonical form (half-open segments `[lower, upper)` of their own kind of version),
/// so equal sets compare equal, as the solver needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    stable: pubgrub::Ranges<Version>,
    pre: pubgrub::Ranges<Version>,
}

/// `MAJOR.MINOR.PATCH-0`, the lowest pre-release of a version.
fn floor(major: u64, minor: u64, patch: u64) -> Version {
    let mut v = Version::new(major, minor, patch);
    v.pre = Prerelease::new("0").expect("0 is a valid pre-release");
    v
}
fn release_of(v: &Version) -> Version {
    Version::new(v.major, v.minor, v.patch)
}
/// The next release after the release `v`, `None` past the largest one.
fn next_release(v: &Version) -> Option<Version> {
    Some(Version::new(v.major, v.minor, v.patch.checked_add(1)?))
}
/// The pre-release right after the pre-release `v` (`1.0.0-beta` → `1.0.0-beta.0`).
fn next_pre(v: &Version) -> Version {
    let mut next = v.clone();
    next.pre = Prerelease::new(&format!("{}.0", v.pre)).expect("valid pre-release");
    next
}

/// Canonical segments of one lane: `Included` lower and `Excluded` (or no) upper bounds
/// that are versions of that lane, empty segments dropped.
fn canonical(ranges: &pubgrub::Ranges<Version>, pre: bool) -> pubgrub::Ranges<Version> {
    let mut result = pubgrub::Ranges::empty();
    for (lower, upper) in ranges.iter() {
        // The smallest version of the lane the lower bound admits.
        let low = match (lower, pre) {
            (Bound::Unbounded, false) => Some(Version::new(0, 0, 0)),
            (Bound::Unbounded, true) => Some(floor(0, 0, 0)),
            (Bound::Included(v), false) if v.pre.is_empty() => Some(v.clone()),
            (Bound::Excluded(v), false) if v.pre.is_empty() => next_release(v),
            (Bound::Included(v) | Bound::Excluded(v), false) => Some(release_of(v)),
            (Bound::Included(v), true) if !v.pre.is_empty() => Some(v.clone()),
            (Bound::Excluded(v), true) if !v.pre.is_empty() => Some(next_pre(v)),
            (Bound::Included(v) | Bound::Excluded(v), true) => {
                next_release(v).map(|n| floor(n.major, n.minor, n.patch))
            }
        };
        let Some(low) = low else {
            continue;
        };
        // The smallest version of the lane above everything the upper bound admits.
        let high = match (upper, pre) {
            (Bound::Unbounded, _) => None,
            (Bound::Excluded(v), false) if v.pre.is_empty() => Some(v.clone()),
            (Bound::Included(v), false) if v.pre.is_empty() => next_release(v),
            (Bound::Included(v) | Bound::Excluded(v), false) => Some(release_of(v)),
            (Bound::Excluded(v), true) if !v.pre.is_empty() => Some(v.clone()),
            (Bound::Included(v), true) if !v.pre.is_empty() => Some(next_pre(v)),
            (Bound::Included(v) | Bound::Excluded(v), true) => {
                next_release(v).map(|n| floor(n.major, n.minor, n.patch))
            }
        };
        let segment = match high {
            Some(high) if high <= low => continue,
            Some(high) => pubgrub::Ranges::between(low, high),
            None => pubgrub::Ranges::higher_than(low),
        };
        result = result.union(&segment);
    }
    result
}

impl Range {
    fn new(stable: &pubgrub::Ranges<Version>, pre: &pubgrub::Ranges<Version>) -> Self {
        Range {
            stable: canonical(stable, false),
            pre: canonical(pre, true),
        }
    }
    /// The same interval in both lanes.
    fn both(ranges: &pubgrub::Ranges<Version>) -> Self {
        Self::new(ranges, ranges)
    }
    pub fn empty() -> Self {
        Self::both(&pubgrub::Ranges::empty())
    }
    pub fn full() -> Self {
        Self::both(&pubgrub::Ranges::full())
    }
    pub fn singleton(v: Version) -> Self {
        Self::both(&pubgrub::Ranges::singleton(v))
    }
    /// Every version (release or pre-release) at or above `v`.
    pub fn higher_than(v: Version) -> Self {
        Self::both(&pubgrub::Ranges::higher_than(v))
    }
    pub fn contains(&self, v: &Version) -> bool {
        match v.pre.is_empty() {
            true => self.stable.contains(v),
            false => self.pre.contains(v),
        }
    }
    pub fn complement(&self) -> Self {
        Self::new(&self.stable.complement(), &self.pre.complement())
    }
    pub fn intersection(&self, other: &Self) -> Self {
        Self::new(
            &self.stable.intersection(&other.stable),
            &self.pre.intersection(&other.pre),
        )
    }
    pub fn union(&self, other: &Self) -> Self {
        Self::new(
            &self.stable.union(&other.stable),
            &self.pre.union(&other.pre),
        )
    }
}

impl pubgrub::VersionSet for Range {
    type V = Version;
    fn empty() -> Self {
        Range::empty()
    }
    fn singleton(v: Version) -> Self {
        Range::singleton(v)
    }
    fn complement(&self) -> Self {
        Range::complement(self)
    }
    fn intersection(&self, other: &Self) -> Self {
        Range::intersection(self, other)
    }
    fn contains(&self, v: &Version) -> bool {
        Range::contains(self, v)
    }
    fn full() -> Self {
        Range::full()
    }
    fn union(&self, other: &Self) -> Self {
        Range::union(self, other)
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn lane(ranges: &pubgrub::Ranges<Version>, pre: bool) -> Vec<String> {
            let lowest = match pre {
                true => floor(0, 0, 0),
                false => Version::new(0, 0, 0),
            };
            ranges
                .iter()
                .map(|(lower, upper)| {
                    let Bound::Included(low) = lower else {
                        return format!("{lower:?} {upper:?}");
                    };
                    let single = match pre {
                        true => Some(next_pre(low)),
                        false => next_release(low),
                    };
                    match upper {
                        Bound::Excluded(high) if Some(high) == single.as_ref() => low.to_string(),
                        Bound::Unbounded if *low == lowest => "*".into(),
                        Bound::Unbounded => format!(">={low}"),
                        Bound::Excluded(high) if *low == lowest => format!("<{high}"),
                        Bound::Excluded(high) => format!(">={low} <{high}"),
                        Bound::Included(high) => format!(">={low} <={high}"),
                    }
                })
                .collect()
        }
        let stable = lane(&self.stable, false);
        let pre = lane(&self.pre, true);
        match (stable.is_empty(), pre.is_empty()) {
            (true, true) => f.write_str("no version"),
            (false, true) => f.write_str(&stable.join(" or ")),
            (true, false) => write!(f, "pre-release {}", pre.join(" or ")),
            (false, false) => write!(
                f,
                "{} or pre-release {}",
                stable.join(" or "),
                pre.join(" or ")
            ),
        }
    }
}

/// A package version (see [`catalog::version_key`]).
pub fn version(text: &str) -> Result<Version> {
    catalog::version_key(text)
}

/// Does a selection consider `candidate` for a package at `installed` (`None`: nothing
/// installed)? Releases always; a pre-release only when the installed version is a
/// pre-release of the same `MAJOR.MINOR.PATCH` (an opted-in package follows its
/// pre-release track up to the release).
pub fn follows(installed: Option<&Version>, candidate: &Version) -> bool {
    candidate.pre.is_empty()
        || installed.is_some_and(|installed| {
            !installed.pre.is_empty()
                && (installed.major, installed.minor, installed.patch)
                    == (candidate.major, candidate.minor, candidate.patch)
        })
}

/// Is moving from `from` to `to` a major update: `to` is newer and outside `^from`
/// (a new MAJOR; for 0.x a new MINOR; for 0.0.x any newer version)?
pub fn is_major(from: &Version, to: &Version) -> bool {
    to > from
        && (to.major != from.major
            || (from.major == 0
                && (to.minor != from.minor || (from.minor == 0 && to.patch != from.patch))))
}

/// [`is_major`] on version texts; unparsable input is never major.
pub fn is_major_text(from: &str, to: &str) -> bool {
    matches!((version(from), version(to)), (Ok(from), Ok(to)) if is_major(&from, &to))
}

/// A version as written in a range: 1 to 3 numbers, a pre-release only with all three.
struct Partial {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    /// The full version when all three numbers are given.
    full: Option<Version>,
}

fn partial(text: &str) -> std::result::Result<Partial, String> {
    let bad = || format!("{text:?} is not a version");
    if text.contains('-') || text.split('.').count() == 3 {
        let full = catalog::version_key(text).map_err(|_| bad())?;
        return Ok(Partial {
            major: full.major,
            minor: Some(full.minor),
            patch: Some(full.patch),
            full: Some(full),
        });
    }
    let numbers = text
        .split('.')
        .map(|part| {
            let valid = !part.is_empty()
                && part.bytes().all(|c| c.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'));
            valid
                .then(|| part.parse::<u64>().ok())
                .flatten()
                .ok_or_else(bad)
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match numbers[..] {
        [major] => Ok(Partial {
            major,
            minor: None,
            patch: None,
            full: None,
        }),
        [major, minor] => Ok(Partial {
            major,
            minor: Some(minor),
            patch: None,
            full: None,
        }),
        _ => Err(bad()),
    }
}

const OPERATORS: [&str; 7] = ["<=", ">=", "<", ">", "=", "^", "~"];

/// One comparator as an interval of the full SemVer order, and the release whose
/// pre-releases it names (if it names one).
fn comparator(
    token: &str,
) -> std::result::Result<(pubgrub::Ranges<Version>, Option<Version>), String> {
    use pubgrub::Ranges;
    let (op, rest) = OPERATORS
        .iter()
        .find_map(|op| token.strip_prefix(op).map(|rest| (*op, rest)))
        .unwrap_or(("", token));
    if rest.contains(['x', 'X', '*']) {
        return Err(format!("wildcard {token:?} is not supported; use ^ or ~"));
    }
    let p = partial(rest)?;
    let named = p
        .full
        .as_ref()
        .filter(|v| !v.pre.is_empty())
        .map(release_of);
    let lower = p
        .full
        .clone()
        .unwrap_or_else(|| Version::new(p.major, p.minor.unwrap_or(0), p.patch.unwrap_or(0)));
    let bump = |n: u64| {
        n.checked_add(1)
            .ok_or_else(|| format!("{token:?} is out of range"))
    };
    // The first version after everything a partial version spells out.
    let after = || -> std::result::Result<Version, String> {
        Ok(match (p.minor, p.patch) {
            (None, _) => floor(bump(p.major)?, 0, 0),
            (Some(minor), None) => floor(p.major, bump(minor)?, 0),
            (Some(minor), Some(patch)) => floor(p.major, minor, bump(patch)?),
        })
    };
    let range = match op {
        "" | "=" => match &p.full {
            Some(v) => Ranges::singleton(v.clone()),
            None => {
                return Err(format!(
                    "{token:?} is not a full version; write MAJOR.MINOR.PATCH, ^{rest} or ~{rest}"
                ));
            }
        },
        ">=" => Ranges::higher_than(lower),
        ">" => match &p.full {
            Some(v) => Ranges::strictly_higher_than(v.clone()),
            None => Ranges::higher_than(after()?),
        },
        "<" => match &p.full {
            Some(v) => Ranges::strictly_lower_than(v.clone()),
            None => Ranges::strictly_lower_than(floor(lower.major, lower.minor, lower.patch)),
        },
        "<=" => match &p.full {
            Some(v) => Ranges::lower_than(v.clone()),
            None => Ranges::strictly_lower_than(after()?),
        },
        "~" => {
            let upper = match p.minor {
                None => floor(bump(p.major)?, 0, 0),
                Some(minor) => floor(p.major, bump(minor)?, 0),
            };
            Ranges::between(lower, upper)
        }
        "^" => {
            let upper = match (p.major, p.minor, p.patch) {
                (0, Some(0), Some(patch)) => floor(0, 0, bump(patch)?),
                (0, Some(minor), _) => floor(0, bump(minor)?, 0),
                (major, _, _) => floor(bump(major)?, 0, 0),
            };
            Ranges::between(lower, upper)
        }
        _ => unreachable!("every operator is matched"),
    };
    Ok((range, named))
}

/// Parse an npm-style requirement into the versions it allows: a bare full version is
/// exact (`1.2.3`, also `=1.2.3`), `^1.2.3` compatible (0.x: `^0.2.3` = `>=0.2.3
/// <0.3.0`, `^0.0.3` = only 0.0.3), `~1.2.3` patch-level, the comparators `>`, `>=`,
/// `<`, `<=` separated by spaces are AND, `*` any release. A pre-release is allowed
/// only when a comparator names a pre-release of its `MAJOR.MINOR.PATCH`. Commas,
/// `||`, hyphen ranges and other wildcards are refused with `invalid_manifest`.
pub fn requirement(text: &str) -> Result<Range> {
    let invalid = |reason: String| {
        Error::invalid_manifest(format!("invalid version requirement {text:?}: {reason}"))
    };
    if text.contains(',') {
        return Err(invalid(
            "commas are not supported; separate comparators with spaces (>=1.3.0 <2.0.0)".into(),
        ));
    }
    if text.contains("||") {
        return Err(invalid("|| alternatives are not supported".into()));
    }
    // An operator written apart from its version (`>= 1.2.3`) belongs to it.
    let mut tokens: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match tokens.last_mut() {
            Some(last) if OPERATORS.contains(&last.as_str()) => last.push_str(word),
            _ => tokens.push(word.to_owned()),
        }
    }
    if tokens.is_empty() {
        return Err(invalid("empty requirement".into()));
    }
    if tokens.iter().any(|t| t == "-") {
        return Err(invalid(
            "hyphen ranges are not supported; write >=A <=B".into(),
        ));
    }
    if tokens == ["*"] {
        return Ok(Range::new(
            &pubgrub::Ranges::full(),
            &pubgrub::Ranges::empty(),
        ));
    }
    let mut all = pubgrub::Ranges::full();
    let mut windows = pubgrub::Ranges::empty();
    for token in &tokens {
        let (range, named) = comparator(token).map_err(invalid)?;
        all = all.intersection(&range);
        if let Some(release) = named {
            let low = floor(release.major, release.minor, release.patch);
            windows = windows.union(&pubgrub::Ranges::between(low, release));
        }
    }
    Ok(Range::new(&all, &all.intersection(&windows)))
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

/// Consumers that break when `provider_id` stops providing (`after` is the state once
/// it is disabled or removed and the bindings it provides are dropped), sorted: every
/// installed package (other than `provider_id`) with a dependency that resolves to
/// `provider_id` in `before` and is not satisfied in `after`. A dependency resolving to
/// it directly always breaks; one resolving to it through a replacement breaks unless
/// the original takes it over (installed, enabled, matching version).
pub(crate) fn breaking_dependents(before: &State, provider_id: &str, after: &State) -> Vec<String> {
    before
        .packages
        .iter()
        .filter(|(id, plugin)| {
            id.as_str() != provider_id
                && plugin.dependencies.keys().any(|dep| {
                    provider(before, dep).is_some_and(|(p, _, _)| p == provider_id)
                        && after.packages.get(id.as_str()).is_none_or(|consumer| {
                            status(after, consumer)
                                .iter()
                                .any(|row| row.id == *dep && !row.satisfied)
                        })
                })
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
            // A refusal from a package's own manifest names that package.
            Package::Id(id) => match self.fixed.get(id) {
                Some(fixed) => self.constraints(&fixed.dependencies),
                None => self
                    .catalog
                    .ok_or_else(|| Error::Catalog("no catalog configured".into()))
                    .and_then(|location| catalog::lookup(location, id, Some(&version.to_string())))
                    .and_then(|release| self.constraints(&release.manifest.dependencies)),
            }
            .map_err(|error| error.with_package(id))?,
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
    fn requirements_follow_npm_semantics() {
        // A bare full version is exact.
        allows("1.2.3", &["1.2.3"], &["1.2.4", "1.2.2", "1.9.0"]);
        allows("=1.0.0", &["1.0.0"], &["1.0.1", "0.9.9"]);
        // Caret, with the 0.x rules.
        allows("^1.2.3", &["1.2.3", "1.9.0"], &["1.2.2", "2.0.0"]);
        allows("^1.2", &["1.2.0", "1.99.0"], &["1.1.9", "2.0.0"]);
        allows("^1", &["1.0.0", "1.99.0"], &["0.9.0", "2.0.0"]);
        allows("^0.2.3", &["0.2.3", "0.2.9"], &["0.3.0", "0.2.2"]);
        allows("^0.2", &["0.2.0", "0.2.5"], &["0.3.0", "0.1.9"]);
        allows("^0.0.3", &["0.0.3"], &["0.0.4", "0.0.2", "0.1.0"]);
        allows("^0.0", &["0.0.0", "0.0.9"], &["0.1.0"]);
        allows("^0", &["0.0.0", "0.9.9"], &["1.0.0"]);
        // Tilde.
        allows("~1.2.3", &["1.2.3", "1.2.9"], &["1.3.0", "1.2.2"]);
        allows("~1.2", &["1.2.0", "1.2.9"], &["1.3.0"]);
        allows("~1", &["1.0.0", "1.9.0"], &["2.0.0"]);
        allows("~0.2.3", &["0.2.3", "0.2.9"], &["0.3.0"]);
        // Comparators, space-separated AND, partial versions as npm.
        allows(">=1.3.0 <2.0.0", &["1.3.0", "1.9.9"], &["1.2.9", "2.0.0"]);
        allows(">= 1.3.0  < 2.0.0", &["1.3.0"], &["2.0.0"]);
        allows(">=1.3", &["1.3.0", "9.0.0"], &["1.2.9"]);
        allows(">1.2.3", &["1.2.4"], &["1.2.3"]);
        allows(">1.2", &["1.3.0"], &["1.2.9"]);
        allows("<2", &["1.9.9"], &["2.0.0"]);
        allows("<=1.2", &["1.2.9"], &["1.3.0"]);
        allows("<=1.2.3", &["1.2.3"], &["1.2.4"]);
        allows(
            ">=1.0.0 <1.5.0 >1.2.0",
            &["1.2.1", "1.4.9"],
            &["1.2.0", "1.5.0"],
        );
        allows("*", &["0.0.1", "99.0.0"], &[]);
        // Pre-releases only through a comparator naming one of the same version.
        allows("*", &[], &["1.0.0-beta.1"]);
        allows(
            "^1.2.0",
            &["1.3.0"],
            &["1.3.0-beta.1", "2.0.0-rc.1", "1.2.0-beta"],
        );
        allows(">=1.0.0 <2.0.0", &[], &["1.5.0-beta", "2.0.0-0"]);
        allows(
            "^1.3.0-beta.1",
            &[
                "1.3.0-beta.1",
                "1.3.0-beta.2",
                "1.3.0-rc.1",
                "1.3.0",
                "1.4.0",
            ],
            &["1.3.0-alpha", "1.4.0-beta.1", "2.0.0", "2.0.0-beta"],
        );
        allows("^1.3.0-beta", &["1.3.0-beta.1"], &["1.3.0-alpha"]);
        allows(
            "1.3.0-beta.1",
            &["1.3.0-beta.1"],
            &["1.3.0-beta.2", "1.3.0"],
        );
        allows(
            ">1.3.0-beta.1",
            &["1.3.0-beta.2", "1.3.0"],
            &["1.3.0-beta.1", "1.4.0-beta"],
        );
        allows(
            "~2.0.0-rc.1",
            &["2.0.0-rc.2", "2.0.0", "2.0.5"],
            &["2.1.0", "2.0.1-rc.1"],
        );
        allows(
            ">=1.0.0-beta <2.0.0-beta",
            &["1.0.0-beta.3", "1.5.0", "2.0.0-alpha"],
            &["1.5.0-beta", "2.0.0-beta", "2.0.0"],
        );
        // Refused syntaxes: Cargo commas, alternatives, hyphen ranges, wildcards,
        // bare partial versions, build metadata, unknown operators.
        for bad in [
            "",
            " ",
            "banana",
            ">=1.0, <1.5",
            ">=1.3.0, <2.0.0",
            "^1 || ^2",
            "1.0.0 - 2.0.0",
            "1.*",
            "1.x",
            "1.2.x",
            "^1.x",
            "1",
            "1.2",
            "=1.2",
            "1.0.0.0",
            "^1.0.0+build",
            "^1.0-beta",
            "!=1.0.0",
            "==1.0.0",
            "^^1.0.0",
            ">=",
            "v1.0.0",
            "^01.0.0",
        ] {
            assert_eq!(
                requirement(bad).unwrap_err().kind(),
                "invalid_manifest",
                "{bad:?}"
            );
        }
        assert!(matches("^1.0", "1.4.0"));
        assert!(!matches("^1.0", "2.0.0"));
        assert!(!matches("^1.0", "1.0"));
        assert!(matches("^1.3.0-beta.1", "1.3.0-beta.2"));
        assert!(!matches("^1.2.0", "1.3.0-beta.1"));
    }

    #[test]
    fn version_sets_are_canonical_for_the_solver() {
        use pubgrub::VersionSet;
        let v = |text: &str| version(text).unwrap();
        let r = |text: &str| requirement(text).unwrap();
        // Equal sets compare equal whatever bounds produced them.
        assert_eq!(r(">1.2.3"), r(">=1.2.4"));
        assert_eq!(r("<=1.2.3"), r("<1.2.4"));
        assert_eq!(r("^1.2.3"), r(">=1.2.3 <2.0.0"));
        assert_eq!(r("^1.2.0").intersection(&r("^2")), Range::empty());
        assert!(r("^1.2.0").is_disjoint(&r("^2")));
        assert!(r("^1.5.0").subset_of(&r("^1.2.0")));
        assert_eq!(r("^1.2.0").union(&r("^1.2.0").complement()), Range::full());
        assert_eq!(r("*").complement().complement(), r("*"));
        // A pre-release is in a set only when the set names its version.
        let beta = r("^1.3.0-beta.1");
        assert!(beta.contains(&v("1.3.0-beta.9")));
        assert!(!beta.intersection(&r("^1.2.0")).contains(&v("1.3.0-beta.9")));
        assert!(beta.intersection(&r("^1.2.0")).contains(&v("1.3.0")));
        assert!(Range::higher_than(v("1.3.0-beta.1")).contains(&v("1.4.0-beta.1")));
        assert!(Range::singleton(v("1.3.0-beta.1")).contains(&v("1.3.0-beta.1")));
        assert!(!Range::singleton(v("1.3.0-beta.1")).contains(&v("1.3.0-beta.1.0")));
        assert_eq!(r("1.2.3").to_string(), "1.2.3");
        assert_eq!(r("^1.2").to_string(), ">=1.2.0 <2.0.0");
        assert_eq!(r("*").to_string(), "*");
        assert_eq!(
            r("^1.3.0-beta.1").to_string(),
            ">=1.3.0 <2.0.0 or pre-release >=1.3.0-beta.1 <1.3.1-0"
        );
    }

    #[test]
    fn follows_and_major_mark_the_pre_release_track_and_breaking_updates() {
        let v = |text: &str| version(text).unwrap();
        assert!(follows(None, &v("1.2.0")));
        assert!(!follows(None, &v("1.3.0-beta.1")));
        assert!(!follows(Some(&v("1.2.0")), &v("1.3.0-beta.1")));
        assert!(follows(Some(&v("1.3.0-beta.1")), &v("1.3.0-beta.2")));
        assert!(follows(Some(&v("1.3.0-beta.1")), &v("1.3.0")));
        assert!(!follows(Some(&v("1.3.0-beta.1")), &v("1.4.0-beta.1")));
        assert!(is_major(&v("1.2.3"), &v("2.0.0")));
        assert!(!is_major(&v("1.2.3"), &v("1.9.0")));
        assert!(
            !is_major(&v("2.0.0"), &v("1.2.3")),
            "a downgrade is not major"
        );
        assert!(is_major(&v("0.2.5"), &v("0.3.0")));
        assert!(!is_major(&v("0.2.5"), &v("0.2.6")));
        assert!(is_major(&v("0.0.3"), &v("0.0.4")));
        assert!(!is_major(&v("2.0.0-beta.1"), &v("2.0.0")));
        assert!(is_major(&v("1.9.0"), &v("2.0.0-beta.1")));
        assert!(is_major_text("1.2.3", "2.0.0"));
        assert!(!is_major_text("1.2.3", "banana"));
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
