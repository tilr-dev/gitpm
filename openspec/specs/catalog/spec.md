# catalog Specification

## Purpose

Defines the static JSON catalog: its v2 folder tree (and readable v1 monolith), release records with exact commits and file hashes, generation from Git release tags, immutability of published versions, transport rules, the one-catalog-per-workspace policy and `check`.

## Requirements

### Requirement: Catalog v2 is a tree of static files
A v2 catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a plugin index `namespaces/<ns>/<name>/plugin.json` `{"schema": 2, "id", "source", "versions"}` per plugin (legacy unscoped IDs under `legacy/<id>/`), and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Unknown schema values SHALL be refused with `catalog`.

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `info`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (SHA-256 and size of every `dist/` file), optional `build` `{url, sha256}` (format only; installs come from Git) and, for a package of a multi-package repository, `path`. Namespaced releases MUST have file hashes. Verified by `catalog::validate_release_checks_identity_commit_paths_and_build` and `workspace::release_with_empty_files_is_still_verified`.

#### Scenario: Legacy release without hashes
- **WHEN** a legacy v1-era unscoped release has no `files`
- **THEN** it is still read; only such releases may omit file hashes

#### Scenario: Namespaced release without hashes
- **WHEN** a namespaced release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

### Requirement: v1 catalogs stay readable
A v1 monolithic `catalog.json` SHALL still be readable, and `migrate-catalog` SHALL split a local v1 catalog into the v2 tree without inventing hashes, keeping a backup, refusing invalid catalogs without writing, and printing `Catalog migrated: LOCATION (format v2, one folder per plugin)`. Verified by `migration_preserves_legacy_releases_and_backup` and `catalog::migrate_refuses_invalid_catalogs_without_writing`.

#### Scenario: Migrate
- **WHEN** the user runs `gitpm --catalog ./catalog migrate-catalog` on a v1 catalog
- **THEN** the v2 tree is written and the old file is kept as a backup

### Requirement: Catalog generated from Git release tags
`catalog-update [--sources FILE]` SHALL read the repositories listed in the sources file (one per line, relative to the file; blank and `#` lines skipped), discover every release tag (for multi-package repositories every package in `gitpm.json` at HEAD), pin each tag's commit, hash its `dist/` and write the tree. It SHALL validate every source before writing anything and retain releases of removed sources.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog-update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0
- **THEN** `catalog-update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_generates_all_tags_and_rejects_moved_releases_without_partial_write` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

### Requirement: Published versions are immutable
A published version SHALL NOT change; every version is checked before any write, so a rejected update writes nothing. `register` SHALL compare the whole release JSON except `reference` (and `files` of a legacy record without them) and fail with `version_conflict` ("version ID@V already exists and is immutable"). A plugin ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog-update` regenerates a published version
- **THEN** it copies `reference` and `build` from the published record, compares manifest, source, commit, files and path, and on a difference fails with `version_conflict` ("release ID@V changed; refusing to replace published version")

#### Scenario: Moved tag on regenerate
- **WHEN** a published tag was moved to another commit and the user runs `catalog-update`
- **THEN** it fails with `version_conflict` and no catalog file changes

### Requirement: Broken historical tags are skipped
A broken historical release tag SHALL be skipped and reported (`Warning: SOURCE: reason`), not fail discovery; selecting the latest release, and `catalog-update` itself, SHALL still fail with `version_conflict` ("latest release tag is broken: …") when a skipped tag is newer than every valid release. Verified by `releases::bad_historical_tag_does_not_hide_valid_releases` and `catalog::update_sources_skips_and_reports_a_broken_historical_tag`.

#### Scenario: One bad old tag
- **WHEN** tag 0.1.0's manifest is invalid but 0.2.0 is valid
- **THEN** `catalog-update` publishes 0.2.0 and warns about 0.1.0

### Requirement: Register one version
`register SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` SHALL add one committed version to a local catalog and print the release as JSON; build options MUST be given together and the URL MUST be HTTPS. Verified by `local_catalog_register_read_install_and_immutable_versions` and `invalid_build_metadata_does_not_publish_release`.

#### Scenario: Invalid build metadata
- **WHEN** `--build-url` is `http://…` or the SHA-256 is malformed
- **THEN** register fails and no release is published (an empty root `catalog.json` may already have been created)

### Requirement: Catalog locations and transport
A catalog location SHALL be a local folder, a path to a `*.json` file (normally `catalog.json`), or an HTTP(S) URL. Remote catalogs MUST use HTTPS; `http://` SHALL be accepted only for loopback hosts (`localhost`, loopback IPs, not `localhost.evil.example` or `user@host`). Requests SHALL not follow redirects and SHALL refuse bodies over 16 MiB. HTTP catalogs SHALL be read-only. Verified by `catalog::locations_are_local_paths_or_http_urls`.

#### Scenario: Plain HTTP to a remote host
- **WHEN** the catalog is `http://pkg.example/catalog.json`
- **THEN** every catalog command fails ("catalog URL must use HTTPS (http:// only for localhost)")

### Requirement: Writers are serialized and crash-safe
Catalog writes SHALL hold an exclusive lock file `.<name>.catalog.lock` beside the catalog, use unique temporary files, and clean up leftovers of dead writers; a lock or temp file left by a killed writer MUST NOT block later writers. Verified by `catalog_lock_blocks_writers_and_is_released_after_success` and `stale_catalog_lock_from_a_dead_writer_does_not_block`.

#### Scenario: Concurrent catalog-update
- **WHEN** a second `catalog-update` starts while one is running
- **THEN** it fails with `locked`

### Requirement: Bounded overview
`info` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> info`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 plugins
- **THEN** `info` shows 40 and the limit line

### Requirement: One catalog per workspace
`install_from_catalog` SHALL record the canonical catalog location per plugin (`plugins.<id>.catalog`) and SHALL refuse with `catalog` ("this workspace uses catalog … mixing catalogs is not supported") to install from a different catalog while plugins from another one are installed. There SHALL be no catalog merging, priorities or pinning, and no built-in default catalog. Verified by `one_catalog_per_workspace`.

#### Scenario: Second catalog
- **WHEN** `a` was installed from catalog A and the user installs `b` from catalog B
- **THEN** the install is refused

### Requirement: check compares installed with the catalog
`check` SHALL be read-only (no workspace creation, no lock), read only the catalog entries of installed plugins (latest and installed version) and of fork originals (latest), and report per plugin one status: `up to date`, `update available`, `newer than catalog`, `not in catalog`, `different source in catalog`, `invalid catalog data`, `no versions in catalog`, `version cannot be compared`, `same version, different commit`, `installed from another catalog`.

#### Scenario: Identity mismatch
- **WHEN** one plugin's catalog entry has the wrong ID or version for its location
- **THEN** its row is `invalid catalog data` and the other plugins are reported normally

#### Scenario: Broken payload
- **WHEN** a release record has an invalid commit or hash, invalid JSON, or the catalog cannot be fetched
- **THEN** the whole `check` fails

#### Scenario: CLI table
- **WHEN** the user runs `gitpm check` with installed plugins
- **THEN** the table is coloured only when stdout is a terminal, `NO_COLOR` is unset and `TERM` is not `dumb`, followed by `Replacement: ORIGINAL → FORK (for every consumer)` and `Unsatisfied dependency: ID → DEP REQ: problem` lines

#### Scenario: Other catalog
- **WHEN** a plugin was installed from another catalog than the selected one
- **THEN** its row says `installed from another catalog` and offers no update, unless this catalog has a different source for it or exactly its installed release
- **AND** this is covered by `check::check_reports_invalid_release_without_hiding_healthy_plugins` and `check::read_state_never_creates_or_locks_a_workspace`
