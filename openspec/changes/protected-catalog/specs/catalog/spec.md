## ADDED Requirements

### Requirement: Verified catalogs
A catalog whose top-level `catalog.json` has `"verified": true` (set by its owner by hand) SHALL be verified; any other is unverified and serves discovery only. Writers SHALL keep the flag. A verified catalog SHALL take new versions only through `register`; `catalog-update` into it SHALL fail with `invalid_argument` and write nothing. The ID-to-source binding ("Published versions are immutable") is its namespace ownership. Verified by `catalog::verified_catalog_takes_only_registered_versions`.

#### Scenario: Register into a verified catalog
- **WHEN** the owner set `"verified": true` and runs `register ./greeter --ref acme/greeter@0.2.0`
- **THEN** the version is published and `catalog.json` still has `"verified": true`

#### Scenario: Bulk generation refused
- **WHEN** the user runs `catalog-update` into a verified catalog
- **THEN** it fails with `invalid_argument` ("a verified catalog takes versions only one by one through register") and no file changes

#### Scenario: Another source claims an ID
- **WHEN** `acme/greeter` was registered from `https://git.example/acme/greeter.git` and someone registers `acme/greeter` from another source
- **THEN** register fails with `catalog` ("registered from a different source")

## MODIFIED Requirements

### Requirement: One catalog per workspace
A package installed from a verified catalog, and each dependency pulled from it, SHALL record the catalog's canonical location (`packages.<id>.catalog`) and is protected (updates). Installing from a second verified catalog SHALL be refused with `catalog` ("… mixing verified catalogs is not supported"); unverified catalogs record nothing and are exempt. There SHALL be no catalog merging, priorities, pinning or default catalog. Verified by `one_catalog_per_workspace`.

#### Scenario: Direct install
- **WHEN** a package is installed with `install GIT_SOURCE`
- **THEN** its record has no `catalog`, and its updates never read a catalog (updates, "Updates need no catalog")

#### Scenario: Second catalog
- **WHEN** `a` was installed from verified catalog A and the user installs `b` from verified catalog B
- **THEN** the install is refused; installing `b` from an unverified catalog B succeeds and records no `catalog`

### Requirement: check compares installed packages with their catalog or origin
`check(root, catalog)` (CLI, FFI) SHALL be read-only (opens, locks, creates nothing) and report one row per installed package: from its verified catalog for a protected package, else from the given catalog when it lists the package, else from the recorded origin. Each row SHALL carry a snake_case `CheckStatus`, a message, its `source` (`catalog`/`origin`) and a `problem` for `origin_unreachable`. The report SHALL also list replacements and unsatisfied dependencies. Verified by `check::check_without_catalog_reads_each_origin`.

#### Scenario: Statuses
- **WHEN** `check` reports a package
- **THEN** a catalog row is one of `up_to_date` ("up to date"), `update_available` ("update available"), `newer_than_catalog` ("newer than catalog"), `different_source` ("different source in catalog"), `invalid_catalog_data` ("invalid catalog data"), `no_versions` ("no versions in catalog"), `uncomparable_version` ("version cannot be compared"), `same_version_different_commit` ("same version, different commit") `other_catalog` ("installed from another catalog") or `upstream_not_in_catalog` ("upstream V not in catalog")
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
- **WHEN** `acme/greeter` 0.2.0 is protected by a verified catalog whose latest is 0.2.0 and its origin has the tag `acme/greeter@0.3.0`
- **THEN** its row is `upstream_not_in_catalog` ("upstream 0.3.0 not in catalog") and no update is offered

#### Scenario: Verified or not
- **WHEN** the user runs `gitpm check` with a catalog
- **THEN** the output says whether the catalog is verified (`Catalog: LOCATION (verified)` or `(unverified: discovery only)`), and the report carries it as `catalog_verified`

#### Scenario: Source that is not queried
- **WHEN** a recorded source (or `forkOf.source`) is neither a Git URL nor an absolute path, or names a remote helper (`ext::…`, `fd::5`)
- **THEN** git is not started for it and its row is `origin_unreachable` with `problem` `source not queried: …`; the other rows are reported normally
- **AND** this is covered by `check::unsafe_or_relative_sources_are_never_handed_to_git`

#### Scenario: Exit status
- **WHEN** `gitpm check` finds an update available, or finds none
- **THEN** the exit status is 0 in both cases; only a failed `check` exits 1
