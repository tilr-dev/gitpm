# catalog Specification

## Purpose

Defines the static JSON catalog: its single format, a tree of static files, release records with exact commits and file hashes, generation from Git release tags, immutability of published versions, transport rules, the one-catalog-per-workspace policy, working without a catalog, and `check`.

## Requirements

### Requirement: Catalog is a tree of static files
A catalog SHALL be a tree of JSON files: schema-1 directory links, a schema-1 package index `namespaces/<ns>/<name>/plugin.json` with `id`, `source`, `versions` and `latest`, and one `versions/<V>/release.json` per version. `latest` SHALL summarize the selected release's version and metadata. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

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
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

#### Scenario: Release summary in the index
- **WHEN** a catalog is generated and the latest release of `tilr/theme` has a title and provides
- **THEN** `namespaces/tilr/theme/plugin.json` holds `latest {version, title, description, provides, tags}`, equal to the normalized manifest metadata in its `release.json`; directory files `{"schema": 1, "directories": {…}}` carry only links

#### Scenario: Summary selection and regeneration
- **WHEN** a catalog is generated or a release is registered, including an older version or an already published version
- **THEN** the index summary matches the newest stable release, or the newest prerelease if no stable release exists, and its schema number remains unchanged
- **AND** a failed generation leaves the existing index unchanged

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (every output file’s SHA-256 and size), optional `build` `{url, sha256}` and, for a multi-package repository’s package, `path`. Every release MUST have file hashes. Build SHALL be preferred over Git; its output is the archive root, while Git output is the committed manifest-selected folder (default dist/).

#### Scenario: Legacy release without hashes
- **WHEN** a catalog holds an unscoped release of the v1 era without `files`
- **THEN** reading it fails with `catalog`; no release may omit file hashes

#### Scenario: Namespaced release without hashes
- **WHEN** a namespaced release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

#### Scenario: Declared folder hashes
- **WHEN** a Git release selects its root or a custom output folder
- **THEN** catalog generation and verification hash exactly the same committed files as deployment

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
- **WHEN** a multi-package repository tagged `v0.1.0` with package `tilr/old` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `tilr/old` yet
- **THEN** `catalog update` publishes the releases of the packages listed at HEAD and no release of `tilr/old`

#### Scenario: Duplicate source lines
- **WHEN** the sources file lists the same repository twice, also in another spelling of its path
- **THEN** `catalog update` reads it once and indexes each release once

#### Scenario: Submodule inside dist
- **WHEN** the release contains a Git submodule at `dist/vendor`
- **THEN** publication succeeds, hashing the staged export with an empty `vendor/` directory
- **AND** the author is warned that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

### Requirement: Published versions are immutable
Published versions SHALL be immutable, with tag `vVERSION` and display `ID@VERSION`. Rejected writes SHALL change nothing. `catalog register` SHALL fail with `version_conflict` if release JSON except `reference` differs; `catalog update` SHALL keep differing records and warn. An ID SHALL remain bound to its first source.

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

#### Scenario: Submodule inside dist
- **WHEN** the release contains a Git submodule at `dist/vendor`
- **THEN** publication succeeds, hashing the staged export with an empty `vendor/` directory
- **AND** the author is warned that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

### Requirement: Catalog locations and transport
A catalog location SHALL be a local folder, a path to a `*.json` file (normally `catalog.json`), or an HTTP(S) URL. Remote catalogs MUST use HTTPS; `http://` SHALL be accepted only for loopback hosts (`localhost`, loopback IPs, not `localhost.evil.example` or `user@host`), else `invalid_argument`. Requests SHALL not follow redirects and SHALL refuse a body over 16 MiB, announced or not. HTTP catalogs SHALL be read-only. Verified by `catalog::locations_are_local_paths_or_http_urls`.

#### Scenario: Failed request
- **WHEN** a catalog request fails (connection, timeout, a status other than 200 or 404, a body over 16 MiB)
- **THEN** the command fails with `http`; a 404 means the file is absent

#### Scenario: Streamed body without a length
- **WHEN** an HTTPS catalog server streams a 50 MiB body with chunked encoding and no Content-Length
- **THEN** the request fails with `http` once 16 MiB have been received, and nothing is parsed or written

#### Scenario: Plain HTTP to a remote host
- **WHEN** the catalog is `http://pkg.example/catalog.json`
- **THEN** every catalog command fails with `invalid_argument` ("catalog URL must use HTTPS (http:// only for localhost)")

#### Scenario: Build redirect to plain HTTP
- **WHEN** a build response redirects to a plain HTTP destination
- **THEN** the request fails with http, even for loopback; catalog redirects remain refused

#### Scenario: Build transport policy
- **WHEN** a build URL is downloaded
- **THEN** the catalog timeouts and 16 MiB streaming bound apply, with at most five HTTPS-only redirects; reading a loopback HTTP build does not relax catalog register's HTTPS-only input

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

### Requirement: One catalog per workspace
A package installed from a verified catalog, and each dependency pulled from it, SHALL record the catalog's canonical location (`packages.<id>.catalog`) and is protected (updates). Installing from a second verified catalog SHALL be refused with `catalog` ("… mixing verified catalogs is not supported"); unverified catalogs record nothing and are exempt. There SHALL be no catalog merging, priorities or pinning, and no built-in default catalog. Verified by `one_catalog_per_workspace`.

#### Scenario: Direct install
- **WHEN** a package is installed with `install GIT_SOURCE`
- **THEN** its record has no `catalog`, and its updates never read a catalog (updates, "Updates need no catalog")

#### Scenario: Second catalog
- **WHEN** `a` was installed from verified catalog A and the user installs `b` from verified catalog B
- **THEN** the install is refused; installing `b` from an unverified catalog B succeeds and records no `catalog`

### Requirement: A catalog is optional
Single source for working without a catalog: a direct install SHALL record its source, and updates and `check` SHALL query that origin (updates). Only catalog commands, `install-catalog` and installing missing dependencies SHALL need a catalog; without one the library's catalog reads fail with `invalid_argument` ("no catalog configured"; CLI message: cli). Verified by `workspace::catalog_operations_need_a_configured_catalog` and `lifecycle_and_three_removal_levels`.

#### Scenario: Lifecycle without a catalog
- **WHEN** no catalog is configured and the user runs `install ./greeter`, `update`, `rollback`, `forget`, `restore` and `uninstall` for `acme/greeter`
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
- **WHEN** `acme/greeter` 0.1.0 was installed from `./greeter`, which now also has the tag `v0.2.0`, and no catalog is given (or the catalog does not list it)
- **THEN** its row is `update_available` with latest `0.2.0` and source `origin`
- **AND** the origin's tag and branch names are read with `git ls-remote`; candidate tags are shallow-fetched with `--depth=1 --filter=blob:none` newest first until a manifest with the requested ID is found, without checking out dist or changing the installed clone or workspace
- **AND** tags are compared by name: an origin tag of the installed version that points to another commit is `up_to_date` (stated limitation; the commit is not compared)

#### Scenario: Recorded modes in check
- **WHEN** one package is pinned and another follows the branch `main`
- **THEN** the pinned row is `pinned` with its ref and needs no origin query, and the branch row compares the installed commit with the HEAD of `main` on the origin (`up_to_date`, or `update_available` when it moved); release-mode packages are compared by release as above

#### Scenario: Protected package with a newer upstream
- **WHEN** Git-installed `acme/greeter` 0.2.0 is protected by a verified catalog whose latest is 0.2.0 and its origin has the tag `v0.3.0`
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

### Requirement: Nothing is built, executed or uploaded
gitpm SHALL only clone Git repositories, copy their package dist folders, download and verify builds, write local catalogs and, for the author operations (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload. Writing a catalog to an HTTP(S) location SHALL fail with `invalid_argument` and a message that names no command.

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
- **WHEN** the owner set `"verified": true` and runs `catalog register ./greeter --ref v0.2.0`
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
- **WHEN** a package without a catalog is at 1.1.0 and its origin has the tags `v2.0.0-beta.1` and later `v2.0.0`
- **THEN** `check` first shows `up to date; pre-release 2.0.0-beta.1`, then LATEST `2.0.0 (major)` with `update available`

### Requirement: Local release history remains bounded in execution time
For the local integration fixture containing one package with 200 committed releases `1.0.0` through `1.199.0`, catalog generation, catalog overview, catalog installation and direct installation SHALL each complete in less than 60 seconds. This is the local fixture's acceptance budget, not a bound on remote transport latency.

#### Scenario: Two hundred local releases
- **WHEN** the local fixture creates the 200 releases and runs `catalog update`, `catalog show`, `install-catalog` and direct `install` separately
- **THEN** each command completes in less than 60 seconds, the generated catalog contains 200 versions, the overview reports `1.199.0` as latest and 200 versions, and both installation forms select `1.199.0`

### Requirement: Safe prebuilt archive installation
Builds SHALL be tar.gz with output at the root, verified against archive SHA-256 and the exact release file paths, sizes and hashes before commit. Extraction SHALL preserve executable bits, accept only regular files and directories, reject traversal, absolute/duplicate paths, links and special entries, and bound file data by declared sizes and structural data by the response limit. Package code SHALL never run.

#### Scenario: Source unavailable
- **WHEN** a release has a valid build but its source repository is unavailable
- **THEN** install-catalog succeeds from the archive without a clone

#### Scenario: Invalid archive or file hashes
- **WHEN** archive bytes or extracted files do not match the release record
- **THEN** installation fails with catalog and the workspace is unchanged

#### Scenario: Unsafe archive entry
- **WHEN** an archive contains a link, special file or unsafe path
- **THEN** it is refused before commit and cannot write outside staging

### Requirement: Check can bypass catalog release data
`gitpm check --no-catalog` SHALL bypass selected and recorded catalogs for Git-installed packages and check their origins through the partial-fetch fallback. Build installations SHALL keep using their recorded catalog because they have no Git release origin. This option SHALL affect reporting only, without changing installed protection or update policy.

#### Scenario: Force origin despite configured catalog
- **WHEN** the configured catalog lists 1.0.0, the origin has 2.0.0 and a Git-installed package is checked with `--no-catalog`
- **THEN** the row comes from origin with latest 2.0.0, and the selected catalog is not read
