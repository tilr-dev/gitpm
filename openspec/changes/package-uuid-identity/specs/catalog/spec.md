## MODIFIED Requirements

### Requirement: Catalog is a tree of static files
A catalog SHALL be a tree of JSON files: schema-1 directory links, a schema-1 package index `packages/<uuid>/plugin.json` with `id`, `source`, `versions` and `latest`, and one `versions/<V>/release.json` per version. `latest` SHALL summarize the selected release's version and metadata. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog package index has `"schema": 2`
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported package schema" for the index; a `state.json` of another format is `unsupported` instead, see errors)

#### Scenario: v1 catalog
- **WHEN** the catalog location is a monolithic v1 `catalog.json`
- **THEN** every catalog read fails with `catalog` and nothing is written; the user regenerates the catalog with `catalog update`

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `catalog show`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `packages/ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc/` is intact
- **THEN** `install-catalog ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

#### Scenario: Release summary in the index
- **WHEN** a catalog is generated and the latest release of `f851f609-bb81-52d6-bfef-33798624782c` has a title and provides
- **THEN** `packages/f851f609-bb81-52d6-bfef-33798624782c/plugin.json` holds `latest {version, title, description, provides, tags}`, equal to the normalized manifest metadata in its `release.json`; directory files `{"schema": 1, "directories": {…}}` carry only links

#### Scenario: Summary selection and regeneration
- **WHEN** a catalog is generated or a release is registered, including an older version or an already published version
- **THEN** the index summary matches the newest stable release, or the newest prerelease if no stable release exists, and its schema number remains unchanged
- **AND** a failed generation leaves the existing index unchanged

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (every output file’s SHA-256 and size), optional `build` `{url, sha256}` and, for a multi-package repository’s package, `path`. Every release MUST have file hashes.

Verified by `catalog::validate_release_checks_identity_commit_paths_and_build` and `workspace::release_with_empty_files_is_still_verified`.

#### Scenario: Legacy release without hashes
- **WHEN** a catalog holds an unscoped release of the v1 era without `files`
- **THEN** reading it fails with `catalog`; no release may omit file hashes

#### Scenario: UUID release without hashes
- **WHEN** a UUID release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

#### Scenario: Namespaced release without hashes
- **WHEN** a legacy scoped-ID release record has no file hashes
- **THEN** it is refused with `catalog`; neither the old identity nor missing integrity data is migrated

### Requirement: Catalog generated from Git release tags
`catalog update [--sources FILE]` SHALL read listed repositories (paths relative to FILE; blank and comment lines ignored), validate all sources before writing, discover every `vVERSION` release, pin commits, hash dist and write the catalog. Only packages listed at repository HEAD SHALL be published; broken historical tags SHALL be skipped as in package-ids, and releases of removed sources retained.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags `v0.1.0` and `v0.2.0`
- **THEN** `catalog update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_keeps_published_records_when_tags_move` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

#### Scenario: Package removed from gitpm.json at HEAD
- **WHEN** a multi-package repository tagged `v0.1.0` with package `188c5166-1379-5b70-b8e8-dd3947dad886` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `188c5166-1379-5b70-b8e8-dd3947dad886` yet
- **THEN** `catalog update` publishes the releases of the packages listed at HEAD and no release of `188c5166-1379-5b70-b8e8-dd3947dad886`

#### Scenario: Duplicate source lines
- **WHEN** the sources file lists the same repository twice, also in another spelling of its path
- **THEN** `catalog update` reads it once and indexes each release once

#### Scenario: Submodule inside dist
- **WHEN** the release contains a Git submodule at `dist/vendor`
- **THEN** publication succeeds, hashing the staged export with an empty `vendor/` directory
- **AND** the author is warned that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

### Requirement: A catalog is optional
Single source for working without a catalog: a direct install SHALL record its source, and updates and `check` SHALL query that origin (updates). Only catalog commands, `install-catalog` and installing missing dependencies SHALL need a catalog; without one the library's catalog reads fail with `invalid_argument` ("no catalog configured"; CLI message: cli). Verified by `workspace::catalog_operations_need_a_configured_catalog` and `lifecycle_and_three_removal_levels`.

#### Scenario: Lifecycle without a catalog
- **WHEN** no catalog is configured and the user runs `install ./greeter`, `update`, `rollback`, `forget`, `restore` and `uninstall` for `d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** every command succeeds

#### Scenario: check without a catalog
- **WHEN** packages are installed without a catalog and `check` is called without one
- **THEN** it reports every package from its origin and does not fail
- **AND** `browse_catalog`, `catalog_release` and `install_from_catalog` fail with `invalid_argument` ("no catalog configured")

### Requirement: check compares installed packages with their catalog or origin
`check(root, catalog)` (CLI, FFI) SHALL be read-only (no open, lock or creation), reporting one row per package with status, message, `source` and `problem`, plus replacements and unsatisfied dependencies. Git selection SHALL be by mode, verified protection (`other_catalog` under another catalog), selected catalog listing its source, then origin. Builds SHALL read recorded catalogs without Git. Verified by `check::check_without_catalog_reads_each_origin`.

#### Scenario: Statuses
- **WHEN** `check` reports a package
- **THEN** a catalog row is one of `up_to_date` ("up to date"), `update_available` ("update available"), `newer_than_catalog` ("newer than catalog"), `different_source` ("different source in catalog"), `invalid_catalog_data` ("invalid catalog data"), `no_versions` ("no versions in catalog"), `uncomparable_version` ("version cannot be compared"), `same_version_different_commit` ("same version, different commit"), `other_catalog` ("installed from another catalog")
- **AND** an origin row is one of `up_to_date`, `update_available`, `newer_than_origin` ("newer than origin"), `no_releases` ("no releases on origin"), `origin_unreachable` ("origin unreachable", with a `problem` hint, never git's stderr), `uncomparable_version` (malformed installed version) or `pinned` ("pinned")

#### Scenario: Identity mismatch
- **WHEN** one package's catalog entry has the wrong ID or version for its location
- **THEN** its row is `invalid catalog data` and the other packages are reported normally

#### Scenario: Broken payload
- **WHEN** a release record has an invalid commit or hash or invalid JSON, or the catalog cannot be fetched
- **THEN** the whole `check` fails, with `catalog` for invalid data and `http` for a failed request; an unreachable origin only makes its own row `origin_unreachable`

#### Scenario: CLI table
- **WHEN** the user runs `gitpm check` with installed packages
- **THEN** the table is coloured only when stdout is a terminal, `NO_COLOR` is unset and `TERM` is not `dumb`, shows per row whether it came from the catalog or the origin, and is followed by `Replacement: ORIGINAL → FORK (for every consumer)` and `Unsatisfied dependency: ID → DEP REQ: problem` lines taken from the report

#### Scenario: Other catalog
- **WHEN** a package was installed from another catalog than the selected one
- **THEN** its row says `installed from another catalog` and offers no update, unless this catalog has a different source for it or exactly its installed release
- **AND** this is covered by `check_shows_fork_upstream_and_other_catalogs`; the read-only and partial-failure rules by `check::read_state_never_creates_or_locks_a_workspace` and `check::check_reports_invalid_release_without_hiding_healthy_plugins`

#### Scenario: Same ID in an unrelated source
- **WHEN** an unprotected Git-installed package has the same ID as an entry from another source in the selected catalog
- **THEN** `check` checks the installed source through the origin fallback, without suggesting the unrelated catalog release

#### Scenario: Origin row
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.1.0 was installed from `./greeter`, which now also has the tag `v0.2.0`, and no catalog is given (or the catalog does not list it)
- **THEN** its row is `update_available` with latest `0.2.0` and source `origin`
- **AND** the origin's tag and branch names are read with `git ls-remote`; candidate tags are shallow-fetched with `--depth=1 --filter=blob:none` newest first until a manifest with the requested ID is found, without checking out dist or changing the installed clone or workspace
- **AND** tags are compared by name: an origin tag of the installed version that points to another commit is `up_to_date` (stated limitation; the commit is not compared)

#### Scenario: Recorded modes in check
- **WHEN** one package is pinned and another follows the branch `main`
- **THEN** the pinned row is `pinned` with its ref and needs no origin query, and the branch row compares the installed commit with the HEAD of `main` on the origin (`up_to_date`, or `update_available` when it moved); release-mode packages are compared by release as above

#### Scenario: Protected package with a newer upstream
- **WHEN** Git-installed `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.2.0 is protected by a verified catalog whose latest is 0.2.0 and its origin has the tag `v0.3.0`
- **THEN** its row is `up_to_date`, `latest` is the catalog's 0.2.0 and origin is not queried, with or without the catalog explicitly given
- **AND** this is covered by `workspace::check_compares_protected_packages_with_their_catalog`

#### Scenario: Catalog no longer verified
- **WHEN** the owner removed `"verified": true` from the catalog a package was installed from
- **THEN** its row keeps the status its versions give and gets the `problem` `catalog no longer verified`
- **AND** this is covered by `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`

#### Scenario: Verified or not
- **WHEN** the user runs `gitpm check` with a catalog
- **THEN** the output says whether the catalog is verified (`Catalog: LOCATION (verified)` or `Catalog: LOCATION (unverified: discovery only)`), and the report carries it as `catalog_verified` (`false` without a catalog, when the header is `Catalog: none (each package's origin)`, and when the top-level `catalog.json` cannot be read, which never fails the check)

#### Scenario: Source that is not queried
- **WHEN** a recorded source (or `forkOf.source`) is neither a Git URL nor an absolute path, or names a remote helper (`ext::…`, `fd::5`)
- **THEN** git is not started for it and its row is `origin_unreachable` with `problem` `source not queried: …`; the other rows are reported normally
- **AND** this is covered by `check::unsafe_or_relative_sources_are_never_handed_to_git`

#### Scenario: Exit status
- **WHEN** `gitpm check` finds an update available, or finds none
- **THEN** the exit status is 0 in both cases; only a failed `check` exits 1

#### Scenario: Check a build without a selected catalog
- **WHEN** a build package is installed and check runs without a catalog option
- **THEN** it compares the recorded catalog, including an unverified one, without a clone or origin query, and never probes the unavailable Git source

### Requirement: Verified catalogs
A catalog whose top-level `catalog.json` has `"verified": true` SHALL be verified; any other is unverified and serves discovery only. Writers SHALL keep the flag. A verified catalog SHALL take new versions only through `catalog register`; `catalog update` into it SHALL fail with `invalid_argument`. Verified by `catalog::verified_catalog_takes_only_registered_versions` and `verified_catalog_protects_updates_and_detach_drops_it`.

#### Scenario: Register into a verified catalog
- **WHEN** the owner set `"verified": true` and runs `catalog register ./greeter --ref v0.2.0`
- **THEN** the version is published and `catalog.json` still has `"verified": true`

#### Scenario: Bulk generation refused
- **WHEN** the user runs `catalog update` into a verified catalog
- **THEN** it fails with `invalid_argument` ("a verified catalog takes versions only one by one through register") before any source is read, and no file changes; into an unverified catalog `catalog update` works as before

#### Scenario: Overview shows the flag
- **WHEN** the user runs `catalog show` (or a host calls `browse`/FFI `browseCatalog`) on a verified catalog
- **THEN** the overview has `verified` true and `catalog show` prints `Verified: yes (protects the packages installed from it)`

#### Scenario: Another source claims an ID
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` was registered from `https://git.example/acme/greeter.git` and someone registers `d135a7cf-1c63-549a-93cb-8f89656b2ced` from another source
- **THEN** catalog register fails with `catalog` ("registered from a different source"): the ID-to-source binding ("Published versions are immutable") is the UUID-to-source ownership of a verified catalog, whose owner sets the flag by hand (no command sets it)

### Requirement: Catalog versions follow SemVer and show pre-releases
Catalog versions SHALL be ordered by SemVer precedence (package-ids, "Version format"); pre-releases are listed by `catalog show` and installable with `--version`. The latest version of a package (`PackageSummary.latest`, an install without `--version`, what `check` compares with) SHALL be its newest version that is not a pre-release (updates, "Pre-releases are opt-in"). A `check` row SHALL carry `major` and `prerelease`. Verified by `catalog::latest_skips_pre_releases_and_check_marks_major`.

#### Scenario: Catalog with a beta
- **WHEN** the catalog has `d135a7cf-1c63-549a-93cb-8f89656b2ced` 1.2.0 and 1.3.0-beta.1 and the user runs `install-catalog d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** 1.2.0 is installed; `install-catalog d135a7cf-1c63-549a-93cb-8f89656b2ced --version 1.3.0-beta.1` installs the beta in release mode, and `update` then follows it to newer pre-releases of 1.3.0 and to the newest release

#### Scenario: check output
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the row says `update available; pre-release 2.1.0-rc.1` with LATEST `2.0.0 (major)`; the row carries `major` true and `prerelease` `2.1.0-rc.1`

#### Scenario: Pre-releases only
- **WHEN** the catalog lists `b4db3752-788f-590f-815a-60e10be71ae5` only at 1.0.0-beta.1
- **THEN** `catalog show` shows `latest: pre-releases only`, `install-catalog b4db3752-788f-590f-815a-60e10be71ae5` fails with `catalog` and `install-catalog b4db3752-788f-590f-815a-60e10be71ae5 --version 1.0.0-beta.1` installs it

#### Scenario: Origin rows
- **WHEN** a package without a catalog is at 1.1.0 and its origin has the tags `v2.0.0-beta.1` and later `v2.0.0`
- **THEN** `check` first shows `up to date; pre-release 2.0.0-beta.1`, then LATEST `2.0.0 (major)` with `update available`
