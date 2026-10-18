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

## REMOVED Requirements

### Requirement: v1 catalogs stay readable
**Reason**: No backward compatibility before 1.0 (D1, TILR-38); v1 monolithic catalogs and `migrate-catalog` are removed.
**Migration**: Regenerate the catalog from Git release tags with `catalog-update` into an empty folder.
