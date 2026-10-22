## MODIFIED Requirements

### Requirement: Catalog v2 is a tree of static files
A catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a catalog package index `namespaces/<ns>/<name>/plugin.json` `{"schema": 2, "id", "source", "versions"}` per package, and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog package index has `"schema": 3`
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported package schema" for the index; a `state.json` of another format is `unsupported` instead, see errors)

#### Scenario: v1 catalog
- **WHEN** the catalog location is a monolithic v1 `catalog.json`
- **THEN** every catalog read fails with `catalog` and nothing is written; the user regenerates the catalog with `catalog update`

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `catalog show`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

### Requirement: Catalog generated from Git release tags
`catalog update [--sources FILE]` SHALL read the repositories listed in the sources file (one per line, relative to the file; blank and `#` lines skipped), discover every release tag, pin each tag's commit, hash its package dist and write the tree. For a multi-package repository it SHALL publish only the packages listed in `gitpm.json` at HEAD. Broken historical tags are skipped as in package-ids. It SHALL validate every source before writing anything and retain releases of removed sources.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0
- **THEN** `catalog update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_keeps_published_records_when_tags_move` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

#### Scenario: Package removed from gitpm.json at HEAD
- **WHEN** a multi-package repository tagged `tilr/old@0.1.0` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `tilr/old` yet
- **THEN** `catalog update` publishes the releases of the packages listed at HEAD and no release of `tilr/old`

#### Scenario: Duplicate source lines
- **WHEN** the sources file lists the same repository twice, also in another spelling of its path
- **THEN** `catalog update` reads it once and indexes each release once

### Requirement: Published versions are immutable
A published version SHALL NOT change; a rejected `catalog register` or `catalog update` writes nothing. `catalog register` SHALL fail with `version_conflict` when the release JSON (except `reference`) differs. `catalog update` SHALL keep a differing published record and warn. A package ID SHALL stay bound to the source that first registered it. Verified by `catalog::published_versions_are_immutable` and `catalog_update_keeps_published_records_when_tags_move`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** catalog register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog update` regenerates a published version
- **THEN** it keeps the published record unchanged (`reference` and `build` included) and never replaces it, whatever differs (manifest, source, commit, files or path)
- **AND** when the tag of a published version now points at another commit, it keeps the record, prints `Warning: release ID@V: tag now points to COMMIT; the published record is kept` and does not fail (package-ids, "Tags are not policed; integrity via protected catalog")

#### Scenario: Manifest or files differ from the published record
- **WHEN** the release commit of a published version is unchanged but its regenerated manifest or file hashes differ from the published record (for example after a manual edit of `release.json`)
- **THEN** `catalog update` keeps the published record, prints `Warning: release ID@V differs from the published record; the published record is kept`, exits 0 and publishes the other versions

#### Scenario: Another source for a registered ID
- **WHEN** a package ID registered from one source is registered or regenerated from another
- **THEN** it fails with `catalog` ("registered from a different source")

### Requirement: Register one version
`catalog register SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` SHALL add one committed version to a local catalog and print the release as JSON; build options MUST be given together (a CLI usage error otherwise) and the build URL MUST be HTTPS with a valid SHA-256 (`catalog` otherwise). Verified by `local_catalog_register_read_install_and_immutable_versions` and `invalid_build_metadata_does_not_publish_release`.

#### Scenario: Invalid build metadata
- **WHEN** `--build-url` is `http://…` or the SHA-256 is malformed
- **THEN** catalog register fails with `catalog` ("build URL must use HTTPS" / "invalid build SHA-256") and no release is published (an empty top-level `catalog.json` may already have been created)

### Requirement: Writers are serialized and crash-safe
Catalog writes SHALL hold an exclusive lock on `.<catalog name>.catalog.lock` beside the catalog, use unique temporary files, and clean up leftovers of dead writers; a lock or temp file left by a killed writer MUST NOT block later writers. Verified by `catalog_lock_blocks_writers_and_is_released_after_success` and `stale_catalog_lock_from_a_dead_writer_does_not_block`.

#### Scenario: Concurrent catalog-update
- **WHEN** a second `catalog update` starts while one is running
- **THEN** it fails with `locked`

### Requirement: Bounded overview
`catalog show` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> catalog show`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 packages
- **THEN** `catalog show` shows 40 and the limit line

#### Scenario: Overview lines
- **WHEN** the user runs `gitpm --catalog ./catalog catalog show` on a local catalog
- **THEN** it prints `Catalog: LOCATION`, `Type: local folder/JSON`, `Verified: yes (protects the packages installed from it)` or `Verified: no (discovery only)`, `Directories and packages:`, the directory tree and for each package `ID — latest: V, versions: N` followed by its `Git: SOURCE` line
- **AND** it changes no file of the catalog or the workspace

### Requirement: Nothing is built, executed or uploaded
gitpm SHALL only clone Git repositories, copy their package dist folders, write local catalogs and, for the author operations (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload. Writing a catalog to an HTTP(S) location SHALL fail with `invalid_argument` and a message that names no command.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog update` against an HTTPS catalog location
- **THEN** the library fails with `invalid_argument` ("writing a catalog requires a local folder; HTTP(S) catalogs are read-only") and nothing is uploaded; the CLI may add a hint naming the command

### Requirement: A configured catalog location must exist
A local catalog location given in `Options.catalog` that does not exist SHALL make `Manager::open_with` fail with `invalid_argument` ("catalog location PATH does not exist") before anything is created; there SHALL be no fallback to "no catalog". Free functions that read a catalog fail the same way. Only the catalog writers (`catalog update`, `catalog register`) create a missing local catalog. Verified by `workspace::a_missing_catalog_location_is_refused_before_creating_anything`.

#### Scenario: Typo in the catalog path
- **WHEN** a host opens a Manager on a new workspace root with `Options.catalog` `./catlog` that does not exist
- **THEN** opening fails with `invalid_argument` and neither the workspace root nor its lock file is created

#### Scenario: New catalog
- **WHEN** the author runs `catalog register` with a local catalog folder that does not exist yet
- **THEN** the folder is created and the release is written

### Requirement: Verified catalogs
A catalog whose top-level `catalog.json` has `"verified": true` SHALL be verified; any other is unverified and serves discovery only. Writers SHALL keep the flag. A verified catalog SHALL take new versions only through `catalog register`; `catalog update` into it SHALL fail with `invalid_argument`. Verified by `catalog::verified_catalog_takes_only_registered_versions` and `verified_catalog_protects_updates_and_detach_drops_it`.

#### Scenario: Register into a verified catalog
- **WHEN** the owner set `"verified": true` and runs `catalog register ./greeter --ref acme/greeter@0.2.0`
- **THEN** the version is published and `catalog.json` still has `"verified": true`

#### Scenario: Bulk generation refused
- **WHEN** the user runs `catalog update` into a verified catalog
- **THEN** it fails with `invalid_argument` ("a verified catalog takes versions only one by one through register") before any source is read, and no file changes; into an unverified catalog `catalog update` works as before

#### Scenario: Overview shows the flag
- **WHEN** the user runs `catalog show` (or a host calls `browse`/FFI `browseCatalog`) on a verified catalog
- **THEN** the overview has `verified` true and `catalog show` prints `Verified: yes (protects the packages installed from it)`

#### Scenario: Another source claims an ID
- **WHEN** `acme/greeter` was registered from `https://git.example/acme/greeter.git` and someone registers `acme/greeter` from another source
- **THEN** catalog register fails with `catalog` ("registered from a different source"): the ID-to-source binding ("Published versions are immutable") is the namespace ownership of a verified catalog, whose owner sets the flag by hand (no command sets it)

### Requirement: Catalog versions follow SemVer and show pre-releases
Catalog versions SHALL be ordered by SemVer precedence (package-ids, "Version format"); pre-releases are listed by `catalog show` and installable with `--version`. The latest version of a package (`PackageSummary.latest`, an install without `--version`, what `check` compares with) SHALL be its newest version that is not a pre-release (updates, "Pre-releases are opt-in"). A `check` row SHALL carry `major` and `prerelease`. Verified by `catalog::latest_skips_pre_releases_and_check_marks_major`.

#### Scenario: Catalog with a beta
- **WHEN** the catalog has `acme/greeter` 1.2.0 and 1.3.0-beta.1 and the user runs `install-catalog acme/greeter`
- **THEN** 1.2.0 is installed; `install-catalog acme/greeter --version 1.3.0-beta.1` installs the beta in release mode, and `update` then follows it to newer pre-releases of 1.3.0 and to the newest release

#### Scenario: check output
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the row says `update available; pre-release 2.1.0-rc.1` with LATEST `2.0.0 (major)`; the row carries `major` true and `prerelease` `2.1.0-rc.1`

#### Scenario: Pre-releases only
- **WHEN** the catalog lists `acme/beta` only at 1.0.0-beta.1
- **THEN** `catalog show` shows `latest: pre-releases only`, `install-catalog acme/beta` fails with `catalog` and `install-catalog acme/beta --version 1.0.0-beta.1` installs it

#### Scenario: Origin rows
- **WHEN** a package without a catalog is at 1.1.0 and its origin has the tags `ID@2.0.0-beta.1` and later `ID@2.0.0`
- **THEN** `check` first shows `up to date; pre-release 2.0.0-beta.1`, then LATEST `2.0.0 (major)` with `update available`

### Requirement: Local release history remains bounded in execution time
For the local integration fixture containing one package with 200 committed releases `1.0.0` through `1.199.0`, catalog generation, catalog overview, catalog installation and direct installation SHALL each complete in less than 60 seconds. This is the local fixture's acceptance budget, not a bound on remote transport latency.

#### Scenario: Two hundred local releases
- **WHEN** the local fixture creates the 200 releases and runs `catalog update`, `catalog show`, `install-catalog` and direct `install` separately
- **THEN** each command completes in less than 60 seconds, the generated catalog contains 200 versions, the overview reports `1.199.0` as latest and 200 versions, and both installation forms select `1.199.0`
