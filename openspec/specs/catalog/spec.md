# catalog Specification

## Purpose

Defines the static JSON catalog: its v2 folder tree (and readable v1 monolith), release records with exact commits and file hashes, generation from Git release tags, immutability of published versions, transport rules, the one-catalog-per-workspace policy, working without a catalog, and `check`.

## Requirements

### Requirement: Catalog v2 is a tree of static files
A v2 catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a catalog plugin index `namespaces/<ns>/<name>/plugin.json` `{"schema": 2, "id", "source", "versions"}` per package (legacy unscoped IDs under `legacy/<id>/`), and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Unknown schema values SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog plugin index has `"schema": 3`
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported plugin schema" for the index; a newer `state.json` format is `unsupported` instead, see errors)

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `info`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (SHA-256 and size of every file of the package dist), optional `build` `{url, sha256}` (format only; installs come from Git) and, for a package of a multi-package repository, `path`. Namespaced releases MUST have file hashes. Verified by `catalog::validate_release_checks_identity_commit_paths_and_build` and `workspace::release_with_empty_files_is_still_verified`.

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
`catalog-update [--sources FILE]` SHALL read the repositories listed in the sources file (one per line, relative to the file; blank and `#` lines skipped), discover every release tag, pin each tag's commit, hash its package dist and write the tree. For a multi-package repository it SHALL publish only the packages listed in `gitpm.json` at HEAD. Broken historical tags are skipped as in package-ids. It SHALL validate every source before writing anything and retain releases of removed sources.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog-update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0
- **THEN** `catalog-update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_generates_all_tags_and_rejects_moved_releases_without_partial_write` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

#### Scenario: Package removed from gitpm.json at HEAD
- **WHEN** a multi-package repository tagged `tilr/old@0.1.0` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `tilr/old` yet
- **THEN** `catalog-update` publishes the releases of the packages listed at HEAD and no release of `tilr/old`

### Requirement: Published versions are immutable
A published version SHALL NOT change; every version is checked before any write, so a rejected update writes nothing. `register` SHALL compare the whole release JSON except `reference` (and `files` of a legacy record without them) and fail with `version_conflict` ("version ID@V already exists and is immutable"). A package ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog-update` regenerates a published version
- **THEN** it copies `reference` and `build` from the published record, compares manifest, source, commit, files and path, and on a difference fails with `version_conflict` ("release ID@V changed; refusing to replace published version")
- **AND** a moved tag of a published version is refused as in package-ids ("Moved release tags are refused")

### Requirement: Register one version
`register SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` SHALL add one committed version to a local catalog and print the release as JSON; build options MUST be given together (a CLI usage error otherwise) and the build URL MUST be HTTPS with a valid SHA-256 (`catalog` otherwise). Verified by `local_catalog_register_read_install_and_immutable_versions` and `invalid_build_metadata_does_not_publish_release`.

#### Scenario: Invalid build metadata
- **WHEN** `--build-url` is `http://…` or the SHA-256 is malformed
- **THEN** register fails with `catalog` ("build URL must use HTTPS" / "invalid build SHA-256") and no release is published (an empty top-level `catalog.json` may already have been created)

### Requirement: Catalog locations and transport
A catalog location SHALL be a local folder, a path to a `*.json` file (normally `catalog.json`), or an HTTP(S) URL. Remote catalogs MUST use HTTPS; `http://` SHALL be accepted only for loopback hosts (`localhost`, loopback IPs, not `localhost.evil.example` or `user@host`), else `invalid_argument`. Requests SHALL not follow redirects and SHALL refuse an announced body over 16 MiB. HTTP catalogs SHALL be read-only. Verified by `catalog::locations_are_local_paths_or_http_urls`.

#### Scenario: Failed request
- **WHEN** a catalog request fails (connection, timeout, a status other than 200 or 404, an announced body over 16 MiB)
- **THEN** the command fails with `http`; a 404 means the file is absent

#### Scenario: Streamed body without a length
- **WHEN** the server streams a body over 16 MiB without Content-Length
- **THEN** it is currently read in full (change `catalog-streaming-size-limit` closes this)

#### Scenario: Plain HTTP to a remote host
- **WHEN** the catalog is `http://pkg.example/catalog.json`
- **THEN** every catalog command fails with `invalid_argument` ("catalog URL must use HTTPS (http:// only for localhost)")

### Requirement: Writers are serialized and crash-safe
Catalog writes SHALL hold an exclusive lock on `.<catalog name>.catalog.lock` beside the catalog, use unique temporary files, and clean up leftovers of dead writers; a lock or temp file left by a killed writer MUST NOT block later writers. Verified by `catalog_lock_blocks_writers_and_is_released_after_success` and `stale_catalog_lock_from_a_dead_writer_does_not_block`.

#### Scenario: Concurrent catalog-update
- **WHEN** a second `catalog-update` starts while one is running
- **THEN** it fails with `locked`

### Requirement: Bounded overview
`info` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> info`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 packages
- **THEN** `info` shows 40 and the limit line

### Requirement: One catalog per workspace
`install_from_catalog`, and installing a missing dependency from the catalog (dependencies), SHALL record the canonical catalog location per package (`plugins.<id>.catalog`) and SHALL refuse with `catalog` ("this workspace uses catalog … mixing catalogs is not supported") to install from a different catalog while packages from another one are installed. There SHALL be no catalog merging, priorities or pinning, and no built-in default catalog. Verified by `one_catalog_per_workspace`.

#### Scenario: Direct install
- **WHEN** a package is installed with `install GIT_SOURCE`
- **THEN** its record has no `catalog`, and its updates never read a catalog (updates, "Updates need no catalog")

#### Scenario: Second catalog
- **WHEN** `a` was installed from catalog A and the user installs `b` from catalog B
- **THEN** the install is refused

### Requirement: A catalog is optional
Single source for working without a catalog: a direct install SHALL record its source and updates SHALL query that origin (updates). Only catalog commands, `install-catalog`, `check` on a non-empty workspace and installing missing dependencies SHALL need a catalog; without one the library's catalog reads fail with `invalid_argument` ("no catalog configured"; CLI message: cli). Verified by `workspace::catalog_operations_need_a_configured_catalog` and `lifecycle_and_three_removal_levels`.

#### Scenario: Lifecycle without a catalog
- **WHEN** no catalog is configured and the user runs `install ./greeter`, `update`, `rollback`, `forget`, `restore` and `uninstall` for `acme/greeter`
- **THEN** every command succeeds

#### Scenario: check without a catalog
- **WHEN** packages are installed without a catalog and the library's `check` is called
- **THEN** it fails with `invalid_argument` ("no catalog configured"); the CLI says `check needs a catalog: …` (see cli)
- **AND** `browse_catalog`, `catalog_release` and `install_from_catalog` fail the same way

### Requirement: check compares installed with the catalog
`check` SHALL be read-only (the CLI `check` and the library function `check::check` create and lock no workspace; FFI `PluginManager.check` opens one, see transactions-locking), read only the catalog entries of installed packages (latest and installed version) and of fork originals (latest), and report per package one status: a stable snake_case `CheckStatus` with a human message.

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
