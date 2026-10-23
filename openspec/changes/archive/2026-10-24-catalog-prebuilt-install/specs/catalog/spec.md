## MODIFIED Requirements

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

### Requirement: Nothing is built, executed or uploaded
gitpm SHALL only clone Git repositories, copy their package dist folders, download and verify builds, write local catalogs and, for the author operations (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload. Writing a catalog to an HTTP(S) location SHALL fail with `invalid_argument` and a message that names no command.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog update` against an HTTPS catalog location
- **THEN** the library fails with `invalid_argument` ("writing a catalog requires a local folder; HTTP(S) catalogs are read-only") and nothing is uploaded; the CLI may add a hint naming the command

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

### Requirement: check compares installed packages with their catalog or origin
`check(root, catalog)` (CLI, FFI) SHALL be read-only (no open, lock or creation), reporting one row per package with status, message, `source` and `problem`, plus replacements and unsatisfied dependencies. Git selection SHALL be by mode, verified protection (`other_catalog` under another catalog), selected catalog listing it, then origin. Builds SHALL read recorded catalogs without Git. Verified by `check::check_without_catalog_reads_each_origin`.

#### Scenario: Statuses
- **WHEN** `check` reports a package
- **THEN** a catalog row is one of `up_to_date` ("up to date"), `update_available` ("update available"), `newer_than_catalog` ("newer than catalog"), `different_source` ("different source in catalog"), `invalid_catalog_data` ("invalid catalog data"), `no_versions` ("no versions in catalog"), `uncomparable_version` ("version cannot be compared"), `same_version_different_commit` ("same version, different commit"), `other_catalog` ("installed from another catalog") or `upstream_not_in_catalog` ("upstream V not in catalog")
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

#### Scenario: Origin row
- **WHEN** `acme/greeter` 0.1.0 was installed from `./greeter`, which now also has the tag `acme/greeter@0.2.0`, and no catalog is given (or the catalog does not list it)
- **THEN** its row is `update_available` with latest `0.2.0` and source `origin`
- **AND** the origin's release tags are read with `git ls-remote` without fetching or changing the clone; the newest `ID@VERSION` tag counts by name, and its manifest is verified only when updating
- **AND** tags are compared by name: an origin tag of the installed version that points to another commit is `up_to_date` (stated limitation; the commit is not compared)

#### Scenario: Recorded modes in check
- **WHEN** one package is pinned and another follows the branch `main`
- **THEN** the pinned row is `pinned` with its ref and needs no origin query, and the branch row compares the installed commit with the HEAD of `main` on the origin (`up_to_date`, or `update_available` when it moved); release-mode packages are compared by release as above

#### Scenario: Protected package with a newer upstream
- **WHEN** Git-installed `acme/greeter` 0.2.0 is protected by a verified catalog whose latest is 0.2.0 and its origin has the tag `acme/greeter@0.3.0`
- **THEN** its row is `upstream_not_in_catalog` ("upstream 0.3.0 not in catalog", the origin's version in `origin_latest`), `latest` is the catalog's 0.2.0 and no update is offered, with or without the catalog given
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

## ADDED Requirements

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
