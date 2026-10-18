## MODIFIED Requirements

### Requirement: Catalog v2 is a tree of static files
A catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a catalog package index `namespaces/<ns>/<name>/plugin.json` `{"schema": 2, "id", "source", "versions"}` per package, and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog package index has `"schema": 3`
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported package schema" for the index; a `state.json` of another format is `unsupported` instead, see errors)

#### Scenario: v1 catalog
- **WHEN** the catalog location is a monolithic v1 `catalog.json`
- **THEN** every catalog read fails with `catalog` and nothing is written; the user regenerates the catalog with `catalog-update`

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `info`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (SHA-256 and size of every file of the package dist), optional `build` `{url, sha256}` (format only; installs come from Git) and, for a package of a multi-package repository, `path`. Every release MUST have file hashes. Verified by `catalog::validate_release_checks_identity_commit_paths_and_build` and `workspace::release_with_empty_files_is_still_verified`.

#### Scenario: Legacy release without hashes
- **WHEN** a catalog holds an unscoped release of the v1 era without `files`
- **THEN** reading it fails with `catalog`; no release may omit file hashes

#### Scenario: Namespaced release without hashes
- **WHEN** a namespaced release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

### Requirement: Published versions are immutable
A published version SHALL NOT change; every version is checked before any write, so a rejected update writes nothing. `register` SHALL compare the whole release JSON except `reference` and fail with `version_conflict` ("version ID@V already exists and is immutable"). A package ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog-update` regenerates a published version
- **THEN** it copies `reference` and `build` from the published record, compares manifest, source, commit, files and path, and on a difference fails with `version_conflict` ("release ID@V changed; refusing to replace published version")
- **AND** a moved tag of a published version is refused as in package-ids ("Moved release tags are refused")

### Requirement: One catalog per workspace
`install_from_catalog`, and installing a missing dependency from the catalog (dependencies), SHALL record the canonical catalog location per package (`packages.<id>.catalog`) and SHALL refuse with `catalog` ("this workspace uses catalog … mixing catalogs is not supported") to install from a different catalog while packages from another one are installed. There SHALL be no catalog merging, priorities or pinning, and no built-in default catalog. Verified by `one_catalog_per_workspace`.

#### Scenario: Direct install
- **WHEN** a package is installed with `install GIT_SOURCE`
- **THEN** its record has no `catalog`, and its updates never read a catalog (updates, "Updates need no catalog")

#### Scenario: Second catalog
- **WHEN** `a` was installed from catalog A and the user installs `b` from catalog B
- **THEN** the install is refused

### Requirement: check compares installed with the catalog
`check` SHALL be read-only (the CLI `check` and the library function `check::check` create and lock no workspace; FFI `PackageManager.check` opens one, see transactions-locking), read only the catalog entries of installed packages (latest and installed version) and of fork originals (latest), and report per package one status: a stable snake_case `CheckStatus` with a human message.

#### Scenario: Statuses
- **WHEN** `check` reports a package
- **THEN** its status is one of `up_to_date` ("up to date"), `update_available` ("update available"), `newer_than_catalog` ("newer than catalog"), `not_in_catalog` ("not in catalog"), `different_source` ("different source in catalog"), `invalid_catalog_data` ("invalid catalog data"), `no_versions` ("no versions in catalog"), `uncomparable_version` ("version cannot be compared"), `same_version_different_commit` ("same version, different commit") or `other_catalog` ("installed from another catalog")

#### Scenario: Identity mismatch
- **WHEN** one package's catalog entry has the wrong ID or version for its location
- **THEN** its row is `invalid catalog data` and the other packages are reported normally

#### Scenario: Broken payload
- **WHEN** a release record has an invalid commit or hash or invalid JSON, or the catalog cannot be fetched
- **THEN** the whole `check` fails, with `catalog` for invalid data and `http` for a failed request

#### Scenario: CLI table
- **WHEN** the user runs `gitpm check` with installed packages
- **THEN** the table is coloured only when stdout is a terminal, `NO_COLOR` is unset and `TERM` is not `dumb`, followed by `Replacement: ORIGINAL → FORK (for every consumer)` and `Unsatisfied dependency: ID → DEP REQ: problem` lines

#### Scenario: Other catalog
- **WHEN** a package was installed from another catalog than the selected one
- **THEN** its row says `installed from another catalog` and offers no update, unless this catalog has a different source for it or exactly its installed release
- **AND** this is covered by `check_shows_fork_upstream_and_other_catalogs`; the read-only and partial-failure rules by `check::read_state_never_creates_or_locks_a_workspace` and `check::check_reports_invalid_release_without_hiding_healthy_plugins`

#### Scenario: Exit status
- **WHEN** `gitpm check` finds an update available, or finds none
- **THEN** the exit status is 0 in both cases; only a failed `check` exits 1

### Requirement: Bounded overview
`info` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> info`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 packages
- **THEN** `info` shows 40 and the limit line

#### Scenario: Overview lines
- **WHEN** the user runs `gitpm --catalog ./catalog info` on a local catalog
- **THEN** it prints `Catalog: LOCATION`, `Type: local folder/JSON`, `Directories and packages:`, the directory tree and for each package `ID — latest: V, versions: N` followed by its `Git: SOURCE` line
- **AND** it changes no file of the catalog or the workspace

## REMOVED Requirements

### Requirement: v1 catalogs stay readable
**Reason**: No backward compatibility before 1.0 (D1, TILR-38); v1 monolithic catalogs and `migrate-catalog` are removed.
**Migration**: Regenerate the catalog from Git release tags with `catalog-update` into an empty folder.
