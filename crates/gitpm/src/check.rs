//! Read-only comparison of installed packages with the selected catalog, or, for a
//! package the catalog does not list (or without a catalog), with the release tags of
//! its recorded origin. A pinned package is reported as pinned; a branch-mode package
//! is compared with the HEAD of its branch on the origin.
use crate::error::{Error, Result};
use crate::{
    ForkOf, Installed, Mode, State,
    catalog::{Catalog, version_key},
    exec,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Outcome of comparing one installed package with the catalog or its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    DifferentSource,
    InvalidCatalogData,
    NoVersions,
    UncomparableVersion,
    SameVersionDifferentCommit,
    UpdateAvailable,
    UpToDate,
    NewerThanCatalog,
    /// Installed from another catalog than the selected one; not compared.
    OtherCatalog,
    /// Origin row: the installed version is newer than every release tag of the origin.
    NewerThanOrigin,
    /// Origin row: the origin has no `ID@VERSION` release tag of the package.
    NoReleases,
    /// Origin row: the origin could not be read; see [`CheckRow::problem`].
    OriginUnreachable,
    /// Pinned to a tag or commit ([`CheckRow::mode`]): not compared, nothing queried.
    Pinned,
    /// Catalog row of a package protected by a verified catalog: the origin has a newer
    /// release ([`CheckRow::origin_latest`]) than every version the catalog lists; no
    /// update is offered.
    UpstreamNotInCatalog,
}
impl CheckStatus {
    /// Human-readable status.
    pub fn message(self) -> &'static str {
        match self {
            CheckStatus::DifferentSource => "different source in catalog",
            CheckStatus::InvalidCatalogData => "invalid catalog data",
            CheckStatus::NoVersions => "no versions in catalog",
            CheckStatus::UncomparableVersion => "version cannot be compared",
            CheckStatus::SameVersionDifferentCommit => "same version, different commit",
            CheckStatus::UpdateAvailable => "update available",
            CheckStatus::UpToDate => "up to date",
            CheckStatus::NewerThanCatalog => "newer than catalog",
            CheckStatus::OtherCatalog => "installed from another catalog",
            CheckStatus::NewerThanOrigin => "newer than origin",
            CheckStatus::NoReleases => "no releases on origin",
            CheckStatus::OriginUnreachable => "origin unreachable",
            CheckStatus::Pinned => "pinned",
            CheckStatus::UpstreamNotInCatalog => "upstream not in catalog",
        }
    }
}

/// Where a row (or a fork's upstream) was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSource {
    /// The selected catalog.
    Catalog,
    /// The release tags of the recorded source (`git ls-remote`, nothing fetched).
    Origin,
}
impl CheckSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckSource::Catalog => "catalog",
            CheckSource::Origin => "origin",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckRow {
    pub id: String,
    pub installed: String,
    /// Latest release in the catalog or on the origin, `None` if unknown.
    pub latest: Option<String>,
    pub enabled: bool,
    pub update_available: bool,
    pub status: CheckStatus,
    /// Whether the row comes from the catalog or from the package's origin.
    pub source: CheckSource,
    /// The recorded mode (omitted in release mode): a pinned row names its ref, a
    /// branch row compares the installed commit with that branch's HEAD on the origin.
    #[serde(skip_serializing_if = "Mode::is_release")]
    pub mode: Mode,
    /// Why the origin could not be read ([`CheckStatus::OriginUnreachable`]); a
    /// classified hint, never git's raw stderr.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The newest release on the origin when the catalog lacks it
    /// ([`CheckStatus::UpstreamNotInCatalog`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_latest: Option<String>,
    /// For a fork: the original's latest release against the fork base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Upstream>,
    /// The available update crosses a breaking boundary (see
    /// [`crate::deps::is_major`]); `false` without an update.
    pub major: bool,
    /// A pre-release newer than `latest` that is not offered (pre-releases are
    /// opt-in); the CLI notes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<String>,
}

impl CheckRow {
    /// Human-readable status of the row: [`CheckStatus::message`], with the origin's
    /// version for [`CheckStatus::UpstreamNotInCatalog`] (`upstream V not in catalog`).
    pub fn message(&self) -> String {
        match (&self.status, &self.origin_latest) {
            (CheckStatus::UpstreamNotInCatalog, Some(version)) => {
                format!("upstream {version} not in catalog")
            }
            (status, _) => status.message().to_owned(),
        }
    }
}

/// Where a fork's original stands: in the catalog when it lists the original,
/// otherwise at `forkOf.source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Upstream {
    pub id: String,
    /// Original version the fork started from (`forkOf.version`).
    pub base: String,
    /// Latest release of the original, `None` if unknown.
    pub latest: Option<String>,
    /// The original has a release newer than the fork base.
    pub newer: bool,
    /// Where `latest` was looked up; `None` when neither the catalog lists the
    /// original nor the fork records `forkOf.source`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CheckSource>,
    /// Why `forkOf.source` could not be read (classified hint, like [`CheckRow::problem`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Read `state.json` without opening (locking or creating) the workspace.
/// A missing workspace means "nothing installed".
pub fn read_state(root: &Path) -> Result<State> {
    let path = root.join("state.json");
    match fs::read(&path) {
        Ok(bytes) => State::parse(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !root.exists() => {
            Ok(State::default())
        }
        Err(e) => Err(Error::Io(format!("cannot read {}: {e}", path.display()))),
    }
}

/// [`compare_in`] without a selected catalog location. Runs `git ls-remote` for every
/// package the catalog does not list (and for fork originals it does not list).
pub fn compare(state: &State, catalog: &Catalog) -> Vec<CheckRow> {
    compare_in(state, catalog, None)
}

/// The newest version of `id` in `catalog` offered to a package at `installed` (no
/// pre-release unless on the installed pre-release's track).
fn latest_version(catalog: &Catalog, id: &str, installed: &str) -> Option<String> {
    crate::catalog::newest_eligible(
        catalog
            .packages
            .get(id)?
            .versions
            .keys()
            .map(String::as_str),
        Some(installed),
    )
    .map(str::to_owned)
}

/// Rows for `state` against the catalog snapshot `catalog` read from `location`
/// (canonical, see [`crate::catalog::canonical_location`]). A package the catalog
/// lists gets a catalog row; one installed from another catalog is reported as
/// [`CheckStatus::OtherCatalog`] and never offered an update, unless this catalog has a
/// different source ([`CheckStatus::DifferentSource`]) or exactly the installed release
/// ([`CheckStatus::UpToDate`]). Every other package gets an origin row from the release
/// tags of its recorded source: this runs `git ls-remote --tags --heads` (bounded, once per
/// source, nothing fetched); a source that is not a URL or an absolute path, or uses a
/// remote helper, is not queried and its row is [`CheckStatus::OriginUnreachable`].
pub fn compare_in(state: &State, catalog: &Catalog, location: Option<&str>) -> Vec<CheckRow> {
    rows(
        state,
        Some((catalog, location)),
        &BTreeMap::new(),
        &BTreeSet::new(),
    )
}

/// One row per installed package, in state order. Each distinct origin is read once.
/// `own` holds the snapshots of the verified catalogs protected packages were installed
/// from, by canonical location, when no catalog is selected.
/// `unverified` are the recorded catalogs whose owner dropped `"verified": true`: their
/// protected rows get the problem [`CATALOG_NO_LONGER_VERIFIED`].
fn rows(
    state: &State,
    catalog: Option<(&Catalog, Option<&str>)>,
    own: &BTreeMap<String, Catalog>,
    unverified: &BTreeSet<String>,
) -> Vec<CheckRow> {
    let mut origins = Origins::default();
    state
        .packages
        .iter()
        .map(|(id, installed)| {
            let protecting = installed
                .catalog
                .as_deref()
                .and_then(|location| own.get(location).map(|c| (c, Some(location))));
            // Compared with the verified catalog that protects it (not another one).
            let own_catalog = installed.catalog.is_some()
                && (protecting.is_some()
                    || catalog.and_then(|(_, location)| location) == installed.catalog.as_deref());
            let mut row = match &installed.mode {
                Mode::Release => protecting
                    .or(catalog)
                    .and_then(|(catalog, location)| catalog_row(id, installed, catalog, location))
                    .map(|row| match own_catalog {
                        true => protected_row(row, id, installed, &mut origins),
                        false => row,
                    })
                    .unwrap_or_else(|| origin_row(id, installed, &mut origins)),
                Mode::Pinned(_) => {
                    let mut row = new_row(id, installed, CheckSource::Origin);
                    row.status = CheckStatus::Pinned;
                    row
                }
                Mode::Branch(branch) => branch_row(id, installed, branch, &mut origins),
            };
            if installed
                .catalog
                .as_ref()
                .is_some_and(|location| unverified.contains(location))
                && row.problem.is_none()
            {
                row.problem = Some(CATALOG_NO_LONGER_VERIFIED.into());
            }
            row.upstream = installed
                .fork_of
                .as_ref()
                .map(|base| upstream(base, catalog.map(|(c, _)| c), &mut origins));
            row
        })
        .collect()
}

fn new_row(id: &str, installed: &Installed, source: CheckSource) -> CheckRow {
    CheckRow {
        id: id.into(),
        installed: installed.current.version.clone(),
        latest: None,
        enabled: installed.enabled,
        update_available: false,
        status: CheckStatus::UpToDate,
        source,
        mode: installed.mode.clone(),
        problem: None,
        origin_latest: None,
        upstream: None,
        major: false,
        prerelease: None,
    }
}

/// A protected package that is up to date with its own verified catalog: when the
/// origin has a newer release than the catalog lists, the row is
/// [`CheckStatus::UpstreamNotInCatalog`] (an unreadable origin changes nothing).
fn protected_row(
    mut row: CheckRow,
    id: &str,
    installed: &Installed,
    origins: &mut Origins,
) -> CheckRow {
    if row.status != CheckStatus::UpToDate {
        return row;
    }
    let top = row.latest.as_deref().unwrap_or(&row.installed);
    if let (Ok(Some(newest)), Ok(top)) = (
        origins.latest(&installed.source, id, &installed.current.version),
        version_key(top),
    ) && version_key(&newest).is_ok_and(|newest| newest > top)
    {
        row.status = CheckStatus::UpstreamNotInCatalog;
        row.origin_latest = Some(newest);
    }
    row
}

/// Status of `latest` against the installed version.
fn versus(row: &mut CheckRow, latest: &str, newer: CheckStatus) {
    let (Ok(latest_key), Ok(current_key)) = (version_key(latest), version_key(&row.installed))
    else {
        row.status = CheckStatus::UncomparableVersion;
        return;
    };
    match latest_key.cmp(&current_key) {
        std::cmp::Ordering::Greater => {
            row.update_available = true;
            row.status = CheckStatus::UpdateAvailable;
            row.major = crate::deps::is_major(&current_key, &latest_key);
        }
        std::cmp::Ordering::Equal => row.status = CheckStatus::UpToDate,
        std::cmp::Ordering::Less => row.status = newer,
    }
}

fn upstream(base: &ForkOf, catalog: Option<&Catalog>, origins: &mut Origins) -> Upstream {
    let mut problem = None;
    let (latest, source) = match catalog.filter(|c| c.packages.contains_key(&base.id)) {
        Some(catalog) => (
            latest_version(catalog, &base.id, &base.version),
            Some(CheckSource::Catalog),
        ),
        None => match &base.source {
            Some(source) => match origins.latest(source, &base.id, &base.version) {
                Ok(latest) => (latest, Some(CheckSource::Origin)),
                Err(reason) => {
                    problem = Some(reason);
                    (None, Some(CheckSource::Origin))
                }
            },
            None => (None, None),
        },
    };
    Upstream {
        id: base.id.clone(),
        base: base.version.clone(),
        newer: latest.as_deref().is_some_and(
            |l| matches!((version_key(l), version_key(&base.version)), (Ok(l), Ok(b)) if l > b),
        ),
        latest,
        source,
        problem,
    }
}

/// Tags and branches per origin, read once per distinct source with `git ls-remote`.
#[derive(Default)]
struct Origins(BTreeMap<String, std::result::Result<exec::RemoteRefs, String>>);
impl Origins {
    /// Tags and branches of `source`; `Err` is the classified reason the origin was
    /// unreadable.
    fn refs(&mut self, source: &str) -> std::result::Result<&exec::RemoteRefs, String> {
        self.0
            .entry(source.to_owned())
            .or_insert_with(|| {
                // Never hand git a source it would run a helper for, or resolve
                // against the temp dir: such a source is not queried at all.
                if let Err(e) = crate::auth::validate_recorded(source) {
                    return Err(format!("source not queried: {e}"));
                }
                exec::ls_remote_refs(source).map_err(|e| {
                    // Classify git's message only, not the command line with the source.
                    let text = e.to_string();
                    let stderr = text.split_once(": ").map_or(text.as_str(), |(_, s)| s);
                    crate::auth::failure(stderr).to_owned()
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
    /// The versions of the `ID@VERSION` tags on `source` by name (their manifests are
    /// verified only when updating); `Err` is the classified reason the origin was
    /// unreadable.
    fn versions(&mut self, source: &str, id: &str) -> std::result::Result<Vec<String>, String> {
        let tags = &self.refs(source)?.tags;
        let prefix = format!("{id}@");
        Ok(tags
            .keys()
            .filter_map(|name| name.strip_prefix(&prefix))
            .filter(|version| version_key(version).is_ok())
            .map(str::to_owned)
            .collect())
    }
    /// Newest of [`Origins::versions`] offered to a package at `installed`.
    fn latest(
        &mut self,
        source: &str,
        id: &str,
        installed: &str,
    ) -> std::result::Result<Option<String>, String> {
        let versions = self.versions(source, id)?;
        Ok(
            crate::catalog::newest_eligible(versions.iter().map(String::as_str), Some(installed))
                .map(str::to_owned),
        )
    }
}

fn origin_row(id: &str, installed: &Installed, origins: &mut Origins) -> CheckRow {
    let mut row = new_row(id, installed, CheckSource::Origin);
    let installed_version = installed.current.version.as_str();
    match origins.versions(&installed.source, id) {
        Err(problem) => {
            row.status = CheckStatus::OriginUnreachable;
            row.problem = Some(problem);
        }
        Ok(versions) => {
            let all = || versions.iter().map(String::as_str);
            match crate::catalog::newest_eligible(all(), Some(installed_version)) {
                // No release tags, or pre-releases the package does not follow only.
                None => row.status = CheckStatus::NoReleases,
                Some(latest) => {
                    versus(&mut row, latest, CheckStatus::NewerThanOrigin);
                    row.latest = Some(latest.to_owned());
                }
            }
            let than = row.latest.as_deref().unwrap_or(installed_version);
            row.prerelease =
                crate::catalog::newest_prerelease(all(), Some(installed_version), than)
                    .map(str::to_owned);
        }
    }
    row
}

/// A branch-mode package: the installed commit against the branch HEAD on the origin
/// (a new commit is an update, with or without a version bump).
fn branch_row(id: &str, installed: &Installed, branch: &str, origins: &mut Origins) -> CheckRow {
    let mut row = new_row(id, installed, CheckSource::Origin);
    match origins.refs(&installed.source) {
        Err(problem) => {
            row.status = CheckStatus::OriginUnreachable;
            row.problem = Some(problem);
        }
        Ok(refs) => match refs.heads.get(branch) {
            None => {
                row.status = CheckStatus::OriginUnreachable;
                row.problem = Some(format!("branch {branch} not found on the origin"));
            }
            Some(head) if *head == installed.current.commit => {}
            Some(_) => {
                row.status = CheckStatus::UpdateAvailable;
                row.update_available = true;
            }
        },
    }
    row
}

/// The catalog row of `id`, `None` when the catalog does not list it (and it was not
/// installed from another catalog): then the origin is asked.
fn catalog_row(
    id: &str,
    installed: &Installed,
    catalog: &Catalog,
    location: Option<&str>,
) -> Option<CheckRow> {
    let other_catalog = matches!(
        (installed.catalog.as_deref(), location),
        (Some(from), Some(selected)) if from != selected
    );
    let mut row = new_row(id, installed, CheckSource::Catalog);
    match catalog.packages.get(id) {
        Some(plugin) => compare_listed(&mut row, id, installed, plugin),
        None if other_catalog => {}
        None => return None,
    }
    // Single selected catalog: a package installed from another catalog is never
    // offered an update from this one. A different source and an identical release are
    // still worth reporting as such.
    if other_catalog
        && (!catalog.packages.contains_key(id)
            || !matches!(
                row.status,
                CheckStatus::DifferentSource | CheckStatus::UpToDate
            ))
    {
        row.status = CheckStatus::OtherCatalog;
        row.update_available = false;
    }
    Some(row)
}

fn compare_listed(
    row: &mut CheckRow,
    id: &str,
    installed: &Installed,
    plugin: &crate::catalog::Package,
) {
    // Do not suggest replacing a local fork with an unrelated repository of the same ID.
    if plugin.source != installed.source {
        row.status = CheckStatus::DifferentSource;
        return;
    }
    let versions = plugin
        .versions
        .iter()
        .map(|(version, release)| {
            let key = version_key(version).ok()?;
            if release.manifest.id != *id
                || release.manifest.version != *version
                || release.source != plugin.source
            {
                return None;
            }
            Some((key, release))
        })
        .collect::<Option<Vec<_>>>();
    let Some(versions) = versions else {
        row.status = CheckStatus::InvalidCatalogData;
        return;
    };
    if versions.is_empty() {
        row.status = CheckStatus::NoVersions;
        return;
    }
    let all = || plugin.versions.keys().map(String::as_str);
    let installed_version = installed.current.version.as_str();
    let Some(latest) = crate::catalog::newest_eligible(all(), Some(installed_version)) else {
        // Only pre-releases the package is not following.
        row.status = CheckStatus::NoVersions;
        row.prerelease =
            crate::catalog::newest_prerelease(all(), Some(installed_version), installed_version)
                .map(str::to_owned);
        return;
    };
    let latest = &plugin.versions[latest];
    row.latest = Some(latest.manifest.version.clone());
    row.prerelease =
        crate::catalog::newest_prerelease(all(), Some(installed_version), &latest.manifest.version)
            .map(str::to_owned);
    if version_key(&installed.current.version).is_ok()
        && let Some(current) = plugin.versions.get(&installed.current.version)
        && current.commit != installed.current.commit
    {
        row.status = CheckStatus::SameVersionDifferentCommit;
        return;
    }
    let latest = latest.manifest.version.clone();
    versus(row, &latest, CheckStatus::NewerThanCatalog);
}

/// What `check` reports: one row per installed package, the replacement bindings and
/// the dependencies that are not satisfied in the workspace.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CheckReport {
    pub rows: Vec<CheckRow>,
    /// The given catalog is verified (its top-level `catalog.json` has
    /// `"verified": true`); `false` without a catalog.
    pub catalog_verified: bool,
    /// `ORIGINAL → FORK` for every consumer.
    pub replacements: BTreeMap<String, String>,
    pub unsatisfied: Vec<UnsatisfiedDependency>,
}
/// A dependency of an installed package the workspace does not satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnsatisfiedDependency {
    /// The installed package that declares the dependency.
    pub package: String,
    pub dependency: String,
    pub requirement: String,
    pub problem: Option<String>,
}

/// Read the workspace state without opening it and report every installed package:
/// from `catalog` when given and it lists the package, otherwise from the release tags
/// of its recorded origin (`git ls-remote`, nothing fetched or written). An empty or
/// missing workspace reads nothing. An unreachable origin only marks its own rows
/// [`CheckStatus::OriginUnreachable`]; a broken or unreadable catalog fails the check.
pub fn check(root: &Path, catalog: Option<&str>) -> Result<CheckReport> {
    let state = read_state(root)?;
    if state.packages.is_empty() {
        return Ok(CheckReport::default());
    }
    let rows = check_state(&state, catalog)?;
    // Display only: an unreadable top-level `catalog.json` (the rows never need it)
    // reports the catalog as unverified instead of failing the check.
    let catalog_verified =
        catalog.is_some_and(|location| crate::catalog::is_verified(location).unwrap_or(false));
    let unsatisfied = state
        .installed()
        .into_iter()
        .flat_map(|package| {
            package
                .dependencies
                .into_iter()
                .filter(|d| !d.satisfied)
                .map(move |d| UnsatisfiedDependency {
                    package: package.id.clone(),
                    dependency: d.id,
                    requirement: d.requirement,
                    problem: d.problem,
                })
        })
        .collect();
    Ok(CheckReport {
        rows,
        catalog_verified,
        replacements: state.replacements,
        unsatisfied,
    })
}
/// [`CheckRow::problem`] of a protected package whose recorded catalog is no longer
/// verified.
pub const CATALOG_NO_LONGER_VERIFIED: &str = "catalog no longer verified";
/// Refusal of a catalog operation without a catalog location.
pub(crate) const NO_CATALOG: &str = "no catalog configured";

/// Rows of `state` against the catalog at `location`; without one, a protected package
/// is compared with the verified catalog it was installed from (read once per location;
/// a broken or unreadable one fails the check like a selected catalog).
pub(crate) fn check_state(state: &State, location: Option<&str>) -> Result<Vec<CheckRow>> {
    // Recorded catalogs whose flag was dropped (an unreadable one is not claimed to be).
    let unverified: BTreeSet<String> = state
        .packages
        .values()
        .filter_map(|p| p.catalog.clone())
        .filter(|recorded| crate::catalog::is_verified(recorded).is_ok_and(|v| !v))
        .collect();
    let Some(location) = location else {
        let mut own = BTreeMap::new();
        for installed in state.packages.values() {
            if let Some(recorded) = &installed.catalog
                && !own.contains_key(recorded)
            {
                own.insert(
                    recorded.clone(),
                    crate::catalog::selected_for_check(recorded, state)?,
                );
            }
        }
        return Ok(rows(state, None, &own, &unverified));
    };
    let snapshot = crate::catalog::selected_for_check(location, state)?;
    let canonical = crate::catalog::canonical_location(location).ok();
    Ok(rows(
        state,
        Some((&snapshot, canonical.as_deref())),
        &BTreeMap::new(),
        &unverified,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Installed, Manifest, Revision,
        catalog::{Package, Release},
    };
    pub(crate) fn fixture() -> (State, Catalog) {
        let installed = Installed {
            // A missing local path: an origin lookup fails fast, without network.
            source: "/nonexistent/pm-check/demo.git".into(),
            current: Revision {
                version: "1.9.0".into(),
                commit: "a".repeat(40),
                ..Default::default()
            },
            previous: None,
            enabled: false,
            ..Default::default()
        };
        let versions = ["1.9.0", "1.10.0"]
            .into_iter()
            .map(|version| {
                (
                    version.into(),
                    Release {
                        manifest: Manifest {
                            id: "vendor/demo".into(),
                            version: version.into(),
                            ..Default::default()
                        },
                        source: installed.source.clone(),
                        commit: if version == "1.9.0" {
                            "a".repeat(40)
                        } else {
                            "b".repeat(40)
                        },
                        reference: format!("vendor/demo@{version}"),
                        files: Default::default(),
                        build: None,
                        path: None,
                    },
                )
            })
            .collect();
        let catalog = Catalog {
            packages: [(
                "vendor/demo".into(),
                Package {
                    source: installed.source.clone(),
                    versions,
                },
            )]
            .into(),
        };
        (
            State {
                packages: [("vendor/demo".into(), installed)].into(),
                ..Default::default()
            },
            catalog,
        )
    }
    #[test]
    fn numeric_latest_includes_disabled_and_does_not_modify_state() {
        let (state, catalog) = fixture();
        let before = serde_json::to_value(&state).unwrap();
        let rows = compare(&state, &catalog);
        assert_eq!(rows[0].latest.as_deref(), Some("1.10.0"));
        assert!(rows[0].update_available && !rows[0].enabled);
        assert_eq!(rows.iter().filter(|r| r.update_available).count(), 1);
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    #[test]
    fn missing_source_and_commit_mismatch_are_not_updates() {
        let (mut state, mut catalog) = fixture();
        // Not listed: the origin is asked (and is unreachable here).
        let row = compare(&state, &Catalog::default()).remove(0);
        assert_eq!(
            (row.status, row.source),
            (CheckStatus::OriginUnreachable, CheckSource::Origin)
        );
        assert!(row.problem.is_some() && !row.update_available);
        catalog.packages.get_mut("vendor/demo").unwrap().source = "another".into();
        assert_eq!(
            compare(&state, &catalog)[0].status.message(),
            "different source in catalog"
        );
        let (_, catalog) = fixture();
        state
            .packages
            .get_mut("vendor/demo")
            .unwrap()
            .current
            .commit = "c".repeat(40);
        let rows = compare(&state, &catalog);
        assert_eq!(rows[0].status.message(), "same version, different commit");
        assert!(!rows[0].update_available);
    }
    #[test]
    fn current_and_ahead_versions() {
        let (mut state, catalog) = fixture();
        let plugin = state.packages.get_mut("vendor/demo").unwrap();
        plugin.current.version = "1.10.0".into();
        plugin.current.commit = "b".repeat(40);
        assert_eq!(compare(&state, &catalog)[0].status.message(), "up to date");
        state
            .packages
            .get_mut("vendor/demo")
            .unwrap()
            .current
            .version = "2.0.0".into();
        assert_eq!(
            compare(&state, &catalog)[0].status.message(),
            "newer than catalog"
        );
    }
    #[test]
    fn plugin_from_another_catalog_is_never_offered_an_update() {
        let (mut state, mut catalog) = fixture();
        state.packages.get_mut("vendor/demo").unwrap().catalog = Some("/catalogs/a".into());
        let row = |state: &State, catalog: &Catalog| {
            compare_in(state, catalog, Some("/catalogs/b"))[0].clone()
        };
        // Same source, newer release in B: not an update from a foreign catalog.
        let r = row(&state, &catalog);
        assert_eq!(
            (r.status, r.update_available),
            (CheckStatus::OtherCatalog, false)
        );
        // Exactly the installed release in B: up to date.
        let plugin = state.packages.get_mut("vendor/demo").unwrap();
        plugin.current.version = "1.10.0".into();
        plugin.current.commit = "b".repeat(40);
        assert_eq!(row(&state, &catalog).status, CheckStatus::UpToDate);
        // Another source in B: reported as such.
        catalog.packages.get_mut("vendor/demo").unwrap().source = "another".into();
        assert_eq!(row(&state, &catalog).status, CheckStatus::DifferentSource);
        // Not in B at all.
        assert_eq!(
            row(&state, &Catalog::default()).status,
            CheckStatus::OtherCatalog
        );
    }
    fn status(state: &State, catalog: &Catalog) -> CheckStatus {
        compare(state, catalog)[0].status
    }

    #[test]
    fn invalid_catalog_entries_are_reported_not_trusted() {
        let (state, catalog) = fixture();
        let mutate: [fn(&mut Release); 4] = [
            |r| r.manifest.id = "other".into(),
            |r| r.manifest.version = "1.10.1".into(),
            |r| r.source = "elsewhere".into(),
            |_| {},
        ];
        for (n, change) in mutate.into_iter().enumerate() {
            let mut catalog = catalog.clone();
            change(
                catalog
                    .packages
                    .get_mut("vendor/demo")
                    .unwrap()
                    .versions
                    .get_mut("1.10.0")
                    .unwrap(),
            );
            let row = &compare(&state, &catalog)[0];
            if n == 3 {
                assert_eq!(row.status, CheckStatus::UpdateAvailable);
            } else {
                assert_eq!(row.status, CheckStatus::InvalidCatalogData, "case {n}");
                assert!(row.latest.is_none() && !row.update_available);
            }
        }
        // A version key that is not MAJOR.MINOR.PATCH invalidates the entry too.
        let mut bad = catalog.clone();
        let plugin = bad.packages.get_mut("vendor/demo").unwrap();
        let mut release = plugin.versions["1.10.0"].clone();
        release.manifest.version = "2.0".into();
        plugin.versions.insert("2.0".into(), release);
        assert_eq!(status(&state, &bad), CheckStatus::InvalidCatalogData);
    }

    #[test]
    fn no_versions_and_uncomparable_installed_version() {
        let (mut state, mut catalog) = fixture();
        catalog
            .packages
            .get_mut("vendor/demo")
            .unwrap()
            .versions
            .clear();
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::NoVersions);
        assert!(row.latest.is_none());

        let (_, catalog) = fixture();
        state
            .packages
            .get_mut("vendor/demo")
            .unwrap()
            .current
            .version = "1.9".into();
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::UncomparableVersion);
        assert_eq!(row.latest.as_deref(), Some("1.10.0"));
        assert!(!row.update_available);
    }

    #[test]
    fn installed_version_missing_from_catalog_still_compares_numerically() {
        let (mut state, catalog) = fixture();
        let current = &mut state.packages.get_mut("vendor/demo").unwrap().current;
        current.version = "1.9.5".into();
        current.commit = "f".repeat(40);
        let row = &compare(&state, &catalog)[0];
        assert_eq!(row.status, CheckStatus::UpdateAvailable);
        assert!(row.update_available);
    }

    #[test]
    fn rows_follow_state_order_and_match_full_ids() {
        let (state, catalog) = fixture();
        let mut state = state;
        let demo = state.packages["vendor/demo"].clone();
        // Same name in another namespace is a different package.
        state.packages.insert("dan/demo".into(), demo.clone());
        state.packages.insert("a/zzz".into(), demo);
        let rows = compare(&state, &catalog);
        let summary: Vec<_> = rows.iter().map(|r| (r.id.as_str(), r.status)).collect();
        assert_eq!(
            summary,
            [
                ("a/zzz", CheckStatus::OriginUnreachable),
                ("dan/demo", CheckStatus::OriginUnreachable),
                ("vendor/demo", CheckStatus::UpdateAvailable),
            ]
        );
        assert_eq!(rows[0].installed, "1.9.0");
        assert!(compare(&State::default(), &catalog).is_empty());
    }

    #[test]
    fn statuses_have_distinct_messages_and_snake_case_names() {
        use std::collections::BTreeSet;
        let all = [
            (CheckStatus::DifferentSource, "different_source"),
            (CheckStatus::InvalidCatalogData, "invalid_catalog_data"),
            (CheckStatus::NoVersions, "no_versions"),
            (CheckStatus::UncomparableVersion, "uncomparable_version"),
            (
                CheckStatus::SameVersionDifferentCommit,
                "same_version_different_commit",
            ),
            (CheckStatus::UpdateAvailable, "update_available"),
            (CheckStatus::UpToDate, "up_to_date"),
            (CheckStatus::NewerThanCatalog, "newer_than_catalog"),
            (CheckStatus::OtherCatalog, "other_catalog"),
            (CheckStatus::NewerThanOrigin, "newer_than_origin"),
            (CheckStatus::NoReleases, "no_releases"),
            (CheckStatus::OriginUnreachable, "origin_unreachable"),
            (CheckStatus::Pinned, "pinned"),
        ];
        for (status, name) in all {
            assert_eq!(serde_json::to_value(status).unwrap(), name);
            assert!(!status.message().is_empty());
        }
        let messages: BTreeSet<_> = all.iter().map(|(s, _)| s.message()).collect();
        assert_eq!(messages.len(), all.len());
        assert_eq!(CheckStatus::UpdateAvailable.message(), "update available");
        let (state, catalog) = fixture();
        let row: CheckRow = compare(&state, &catalog).remove(0);
        let value = serde_json::to_value(row).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "id": "vendor/demo", "installed": "1.9.0", "latest": "1.10.0",
                "enabled": false, "update_available": true, "status": "update_available",
                "source": "catalog", "major": false
            })
        );
        assert_eq!(serde_json::to_value(CheckSource::Origin).unwrap(), "origin");
        assert_eq!(CheckSource::Catalog.as_str(), "catalog");
    }

    #[test]
    fn read_state_never_creates_or_locks_a_workspace() {
        use crate::test_support::TempDir;
        let dir = TempDir::new("check");
        let root = dir.join("workspace");
        assert!(read_state(&root).unwrap().packages.is_empty());
        assert!(!root.exists(), "read_state must not create the workspace");
        // An existing directory without state.json is not an empty workspace.
        fs::create_dir(&root).unwrap();
        assert_eq!(read_state(&root).unwrap_err().kind(), "io");
        fs::write(root.join("state.json"), "{truncated").unwrap();
        let error = read_state(&root).unwrap_err();
        assert_eq!(error.kind(), "invalid_data");
        assert!(
            error
                .to_string()
                .starts_with("invalid workspace state.json")
        );
        // A state.json of another format is refused, never rewritten.
        let old = r#"{"format":2,"plugins":{}}"#;
        fs::write(root.join("state.json"), old).unwrap();
        assert_eq!(read_state(&root).unwrap_err().kind(), "unsupported");
        assert_eq!(fs::read_to_string(root.join("state.json")).unwrap(), old);
        let (state, _) = fixture();
        fs::write(
            root.join("state.json"),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .unwrap();
        assert_eq!(read_state(&root).unwrap().packages.len(), 1);
    }

    #[test]
    fn check_skips_the_catalog_for_an_empty_workspace_and_fails_on_a_bad_catalog() {
        use crate::test_support::TempDir;
        let dir = TempDir::new("check");
        let root = dir.join("workspace");
        // The catalog location is invalid, but it is never read.
        assert!(check(&root, Some("ftp://invalid")).unwrap().rows.is_empty());
        assert!(check(&root, None).unwrap().rows.is_empty());
        fs::create_dir(&root).unwrap();
        fs::write(root.join("state.json"), r#"{"format":3,"packages":{}}"#).unwrap();
        assert!(check(&root, Some("ftp://invalid")).unwrap().rows.is_empty());

        let (state, _) = fixture();
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            check(&root, Some("ftp://invalid")).unwrap_err().kind(),
            "invalid_argument"
        );
        // Without a catalog the origin is asked; an unreachable one is a row, not an error.
        let rows = check(&root, None).unwrap().rows;
        assert_eq!(rows[0].status, CheckStatus::OriginUnreachable);
        // A missing catalog is an error, not "no updates".
        let missing = dir.join("missing-catalog");
        let error = check(&root, Some(missing.to_str().unwrap())).unwrap_err();
        assert_eq!(error.kind(), "invalid_argument");
        assert!(error.to_string().ends_with("does not exist"), "{error}");
    }

    #[test]
    fn check_reports_invalid_release_without_hiding_healthy_plugins() {
        use crate::test_support::{TempDir, release, state_with};
        let dir = TempDir::new("check-invalid");
        let cat = dir.join("catalog");
        let source = "https://example.com/demo.git";
        let mut state = State::default();
        for id in ["dan/broken", "dan/healthy"] {
            crate::catalog::tests::save(&cat, &release(id, "1.0.0", source, 'a'));
            state
                .packages
                .extend(state_with(id, source, "1.0.0", &"a".repeat(40)).packages);
        }
        let path = cat.join("namespaces/dan/broken/versions/1.0.0/release.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["manifest"]["id"] = "dan/wrong".into();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        let rows = check_state(&state, cat.to_str()).unwrap();
        assert_eq!(rows[0].status, CheckStatus::InvalidCatalogData);
        assert_eq!(rows[1].status, CheckStatus::UpToDate);
        assert!(crate::catalog::lookup(cat.to_str().unwrap(), "dan/broken", None).is_err());
        // Directory indexes are unrelated to installed-package diagnostics.
        fs::write(cat.join("catalog.json"), "{ broken").unwrap();
        assert_eq!(check_state(&state, cat.to_str()).unwrap().len(), 2);
        // Unreadable release data must remain a hard error.
        fs::write(
            cat.join("namespaces/dan/broken/versions/1.0.0/release.json"),
            "{ broken",
        )
        .unwrap();
        assert!(check_state(&state, cat.to_str()).is_err());
    }

    #[test]
    fn check_reads_a_local_v2_catalog() {
        use crate::test_support::{TempDir, release};
        let dir = TempDir::new("check");
        let catalog = dir.join("catalog");
        let source = "https://example.com/demo.git";
        for (version, c) in [("1.9.0", 'a'), ("1.10.0", 'b'), ("1.2.0", 'c')] {
            crate::catalog::tests::save(&catalog, &release("dan/demo", version, source, c));
        }
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        let state = crate::test_support::state_with("dan/demo", source, "1.9.0", &"a".repeat(40));
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let report = check(&root, catalog.to_str()).unwrap();
        assert!(report.replacements.is_empty() && report.unsatisfied.is_empty());
        let rows = report.rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, CheckStatus::UpdateAvailable);
        assert_eq!(rows[0].latest.as_deref(), Some("1.10.0"));
    }

    /// Write `state` as the workspace state of `root`.
    fn write_state(root: &Path, state: &State) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("state.json"), serde_json::to_vec(state).unwrap()).unwrap();
    }
    fn installed(source: &str, version: &str) -> Installed {
        crate::test_support::state_with("x/y", source, version, &"a".repeat(40))
            .packages
            .remove("x/y")
            .unwrap()
    }

    #[test]
    fn check_reports_each_recorded_mode() {
        use crate::test_support::{Author, TempDir};
        use crate::{InstallTarget, Manager, Options};
        let dir = TempDir::new("check-modes");
        let root = dir.join("workspace");
        let released = Author::new(dir.join("released"), "acme/released");
        let pinned = Author::new(dir.join("pinned"), "acme/pinned");
        let branch = Author::new(dir.join("branch"), "acme/branch");
        let m = Manager::open_with(&root, Options::default()).unwrap();
        m.install(released.source()).unwrap();
        m.install_package(
            pinned.source(),
            None,
            InstallTarget::Ref("acme/pinned@0.1.0".into()),
        )
        .unwrap();
        m.install_package(branch.source(), None, InstallTarget::Dev)
            .unwrap();
        drop(m);
        let summary = |root: &Path| -> Vec<(String, CheckStatus, Mode)> {
            check(root, None)
                .unwrap()
                .rows
                .into_iter()
                .map(|r| (r.id, r.status, r.mode))
                .collect()
        };
        let pinned_mode = Mode::Pinned("acme/pinned@0.1.0".into());
        let branch_mode = Mode::Branch("main".into());
        assert_eq!(
            summary(&root),
            [
                (
                    "acme/branch".into(),
                    CheckStatus::UpToDate,
                    branch_mode.clone()
                ),
                (
                    "acme/pinned".into(),
                    CheckStatus::Pinned,
                    pinned_mode.clone()
                ),
                ("acme/released".into(), CheckStatus::UpToDate, Mode::Release),
            ]
        );
        // A newer release does not concern the pinned package; a moved branch is an update
        // even without a version bump.
        pinned.release("0.2.0");
        released.release("0.2.0");
        fs::write(branch.path.join("dist/extra.js"), "extra").unwrap();
        crate::test_support::git(&branch.path, &["add", "--all"]);
        crate::test_support::git(&branch.path, &["commit", "-q", "-m", "same version"]);
        assert_eq!(
            summary(&root),
            [
                (
                    "acme/branch".into(),
                    CheckStatus::UpdateAvailable,
                    branch_mode
                ),
                (
                    "acme/pinned".into(),
                    CheckStatus::Pinned,
                    pinned_mode.clone()
                ),
                (
                    "acme/released".into(),
                    CheckStatus::UpdateAvailable,
                    Mode::Release
                ),
            ]
        );
        // A pinned package is never queried: even an unreachable origin is not read.
        let mut state = read_state(&root).unwrap();
        state.packages.get_mut("acme/pinned").unwrap().source = "ext::sh -c id".into();
        let row = rows(&state, None, &BTreeMap::new(), &BTreeSet::new()).remove(1);
        assert_eq!(
            (row.status, row.problem, row.mode),
            (CheckStatus::Pinned, None, pinned_mode)
        );
        // A branch the origin no longer has is reported, not hidden.
        let entry = state.packages.get_mut("acme/branch").unwrap();
        entry.mode = Mode::Branch("gone".into());
        let row = rows(&state, None, &BTreeMap::new(), &BTreeSet::new()).remove(0);
        assert_eq!(row.status, CheckStatus::OriginUnreachable);
        assert_eq!(
            row.problem.as_deref(),
            Some("branch gone not found on the origin")
        );
    }

    #[test]
    fn check_without_catalog_reads_each_origin() {
        use crate::test_support::{Author, TempDir, tree_hash};
        use crate::{Manager, Options};
        let dir = TempDir::new("check-origin");
        let root = dir.join("workspace");
        let greeter = Author::new(dir.join("greeter"), "acme/greeter");
        let m = Manager::open_with(&root, Options::default()).unwrap();
        m.install(greeter.source()).unwrap();
        // A newer release appears on the origin after the install.
        greeter.release("0.2.0");
        drop(m);
        let before = tree_hash(&root);
        let report = check(&root, None).unwrap();
        // Nothing fetched or written: the clone's refs and the workspace are unchanged.
        assert_eq!(tree_hash(&root), before, "check must not fetch or write");
        let row = &report.rows[0];
        assert_eq!(
            (
                row.id.as_str(),
                row.status,
                row.source,
                row.latest.as_deref()
            ),
            (
                "acme/greeter",
                CheckStatus::UpdateAvailable,
                CheckSource::Origin,
                Some("0.2.0")
            )
        );
        assert!(row.update_available && row.problem.is_none());
        // The Manager reads the origin as well when no catalog is configured.
        let m = Manager::open_with(&root, Options::default()).unwrap();
        assert_eq!(m.check().unwrap()[0].status, CheckStatus::UpdateAvailable);
        drop(m);

        // Every origin status, each distinct origin read once.
        let newer = Author::new(dir.join("newer"), "acme/newer");
        let untagged = Author::untagged(dir.join("untagged"), "acme/untagged", "0.1.0");
        // Tags of other packages and non-release names do not count.
        crate::test_support::git(&untagged.path, &["tag", "acme/other@9.0.0"]);
        crate::test_support::git(&untagged.path, &["tag", "v9.0.0"]);
        crate::test_support::git(&untagged.path, &["tag", "acme/untagged@banana"]);
        let state = State {
            packages: [
                ("acme/greeter".into(), installed(greeter.source(), "0.2.0")),
                ("acme/newer".into(), installed(newer.source(), "0.3.0")),
                (
                    "acme/untagged".into(),
                    installed(untagged.source(), "0.1.0"),
                ),
            ]
            .into(),
            ..Default::default()
        };
        let statuses: Vec<_> = rows(&state, None, &BTreeMap::new(), &BTreeSet::new())
            .into_iter()
            .map(|r| (r.id, r.status, r.latest))
            .collect();
        assert_eq!(
            statuses,
            [
                (
                    "acme/greeter".into(),
                    CheckStatus::UpToDate,
                    Some("0.2.0".into())
                ),
                (
                    "acme/newer".into(),
                    CheckStatus::NewerThanOrigin,
                    Some("0.1.0".into())
                ),
                ("acme/untagged".into(), CheckStatus::NoReleases, None),
            ]
        );
    }

    #[test]
    fn check_mixes_catalog_and_origin_rows() {
        use crate::test_support::{Author, TempDir, release};
        let dir = TempDir::new("check-mixed");
        let listed = Author::new(dir.join("listed"), "acme/listed");
        let direct = Author::new(dir.join("direct"), "acme/direct");
        direct.release("0.2.0");
        let cat = dir.join("catalog");
        let mut record = release("acme/listed", "0.1.0", listed.source(), 'a');
        record.commit = listed.head();
        crate::catalog::tests::save(&cat, &record);
        let mut state = State::default();
        state
            .packages
            .insert("acme/listed".into(), installed(listed.source(), "0.1.0"));
        state
            .packages
            .get_mut("acme/listed")
            .unwrap()
            .current
            .commit = listed.head();
        state
            .packages
            .insert("acme/direct".into(), installed(direct.source(), "0.1.0"));
        let root = dir.join("workspace");
        write_state(&root, &state);
        let report = check(&root, cat.to_str()).unwrap();
        let summary: Vec<_> = report
            .rows
            .iter()
            .map(|r| (r.id.as_str(), r.status, r.source))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "acme/direct",
                    CheckStatus::UpdateAvailable,
                    CheckSource::Origin
                ),
                ("acme/listed", CheckStatus::UpToDate, CheckSource::Catalog),
            ]
        );
    }

    #[test]
    fn unreachable_origin_is_a_row_not_a_failure() {
        use crate::test_support::{Author, TempDir};
        let dir = TempDir::new("check-unreachable");
        let healthy = Author::new(dir.join("healthy"), "acme/healthy");
        let gone = dir.join("gone");
        let state = State {
            packages: [
                (
                    "acme/gone".into(),
                    installed(gone.to_str().unwrap(), "0.1.0"),
                ),
                ("acme/healthy".into(), installed(healthy.source(), "0.1.0")),
            ]
            .into(),
            ..Default::default()
        };
        let root = dir.join("workspace");
        write_state(&root, &state);
        let rows = check(&root, None).unwrap().rows;
        assert_eq!(rows[0].status, CheckStatus::OriginUnreachable);
        assert_eq!(rows[0].status.message(), "origin unreachable");
        let problem = rows[0].problem.as_deref().unwrap();
        // A classified hint, never git's stderr (which may contain secrets).
        assert!(problem.starts_with("Repository not found"), "{problem}");
        assert!(rows[0].latest.is_none() && !rows[0].update_available);
        assert_eq!(rows[1].status, CheckStatus::UpToDate);
    }

    #[test]
    fn fork_upstream_from_fork_of_source() {
        use crate::test_support::{Author, TempDir, release};
        let dir = TempDir::new("check-fork");
        let original = Author::new(dir.join("hello"), "acme/hello");
        original.release("1.1.0");
        let fork = Author::new(dir.join("fork"), "me/hello");
        let base = |source: Option<&str>| ForkOf {
            id: "acme/hello".into(),
            version: "0.1.0".into(),
            commit: "a".repeat(40),
            source: source.map(Into::into),
        };
        let mut state = State::default();
        let mut me = installed(fork.source(), "0.1.0");
        me.fork_of = Some(base(Some(original.source())));
        state.packages.insert("me/hello".into(), me);
        let upstream = |state: &State, catalog: Option<&Catalog>| {
            rows(
                state,
                catalog.map(|c| (c, None)),
                &BTreeMap::new(),
                &BTreeSet::new(),
            )[0]
            .upstream
            .clone()
            .unwrap()
        };
        assert_eq!(
            upstream(&state, None),
            Upstream {
                id: "acme/hello".into(),
                base: "0.1.0".into(),
                latest: Some("1.1.0".into()),
                newer: true,
                source: Some(CheckSource::Origin),
                problem: None,
            }
        );
        // A catalog that does not list the original: still the origin.
        assert_eq!(
            upstream(&state, Some(&Catalog::default())).source,
            Some(CheckSource::Origin)
        );
        // A catalog that lists the original wins.
        let cat = dir.join("catalog");
        crate::catalog::tests::save(
            &cat,
            &release("acme/hello", "1.0.0", original.source(), 'b'),
        );
        let snapshot = crate::catalog::selected_for_check(cat.to_str().unwrap(), &state).unwrap();
        let from_catalog = upstream(&state, Some(&snapshot));
        assert_eq!(
            (from_catalog.latest.as_deref(), from_catalog.source),
            (Some("1.0.0"), Some(CheckSource::Catalog))
        );
        // An unreachable forkOf.source keeps the reason.
        state.packages.get_mut("me/hello").unwrap().fork_of =
            Some(base(Some(dir.join("gone").to_str().unwrap())));
        let gone = upstream(&state, None);
        assert_eq!(
            (gone.latest, gone.source),
            (None, Some(CheckSource::Origin))
        );
        assert!(gone.problem.unwrap().starts_with("Repository not found"));
        // Without forkOf.source and catalog the original's latest is unknown.
        state.packages.get_mut("me/hello").unwrap().fork_of = Some(base(None));
        let unknown = upstream(&state, None);
        assert_eq!(
            (unknown.latest, unknown.newer, unknown.source),
            (None, false, None)
        );
    }

    #[test]
    fn unsafe_or_relative_sources_are_never_handed_to_git() {
        use crate::test_support::{TempDir, tree_hash};
        let dir = TempDir::new("check-unsafe");
        let marker = dir.join("pwned");
        let ext = format!("ext::sh -c touch% {}", marker.display());
        let mut state = State::default();
        for (id, source) in [
            ("acme/ext", ext.as_str()),
            ("acme/fd", "fd::5"),
            ("acme/relative", "foo/bar"),
        ] {
            state.packages.insert(id.into(), installed(source, "0.1.0"));
        }
        // A fork whose forkOf.source came from a foreign manifest.
        state.packages.get_mut("acme/relative").unwrap().fork_of = Some(ForkOf {
            id: "acme/original".into(),
            version: "0.1.0".into(),
            commit: "a".repeat(40),
            source: Some(ext.clone()),
        });
        let root = dir.join("workspace");
        write_state(&root, &state);
        let before = tree_hash(&root);
        let rows = check(&root, None).unwrap().rows;
        // "source not queried": refused before any git process was started.
        for row in &rows {
            assert_eq!(row.status, CheckStatus::OriginUnreachable, "{}", row.id);
            assert!(
                row.problem
                    .as_deref()
                    .unwrap()
                    .starts_with("source not queried: "),
                "{:?}",
                row.problem
            );
        }
        let upstream = rows[2].upstream.as_ref().unwrap();
        assert!(
            upstream
                .problem
                .as_deref()
                .unwrap()
                .starts_with("source not queried: ")
        );
        assert!(!marker.exists(), "ext:: helper must never run");
        assert_eq!(tree_hash(&root), before);
    }
}
