## ADDED Requirements

### Requirement: Major updates are marked
An update to a newer version outside the caret range of the installed one (a new MAJOR; for 0.x a new MINOR; for 0.0.x any newer version) SHALL be marked `major` in its `RevisionChange` and in each `update all` entry, and the CLI SHALL append ` (major)` to its line. Authors mark a breaking release only by its version number. A major update is not held back by itself; dependents stay protected by their own requirements. Verified by `workspace::major_updates_are_marked`.

#### Scenario: Major line
- **WHEN** `acme/greeter` 1.2.3 is installed, 2.0.0 is released and nothing depends on it, and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter updated 1.2.3 → 2.0.0 (major)` and the `RevisionChange` has `major` true

#### Scenario: 0.x minor
- **WHEN** a package moves from 0.2.5 to 0.3.0
- **THEN** the change is marked `(major)`; a move from 0.2.5 to 0.2.6 is not, nor one from `2.0.0-beta.1` to `2.0.0`

#### Scenario: A dependent caps the update
- **WHEN** `acme/app` requires `acme/greeter ^1.2.0` and the user runs `update acme/greeter` with 2.0.0 released
- **THEN** it fails with `dependency_conflict` and the workspace is unchanged (`cycles_resolve_and_updates_keep_dependents_working`)

### Requirement: Pre-releases are opt-in
Install, update and `update all` SHALL never pick a pre-release implicitly. A pre-release SHALL be installed only when named (`--ref ID@1.3.0-beta.1`, `install-catalog ID --version 1.3.0-beta.1`, a dependency range naming it). Once one is installed in release mode, `update` and `update all` SHALL also consider newer pre-releases of the same MAJOR.MINOR.PATCH besides every newer release. A pre-release tag not considered never blocks an update. Verified by `workspace::pre_releases_are_opt_in`.

#### Scenario: Newest tag is a pre-release
- **WHEN** a package is at 1.2.0 and the origin also has `1.3.0-beta.1`
- **THEN** `update` reports `up to date (1.2.0)`

#### Scenario: A ref install is pinned
- **WHEN** the user installs with `--ref ID@1.3.0-beta.1` and later runs `update ID --release`
- **THEN** the package stays pinned until then; in release mode it follows the 1.3.0 track

#### Scenario: Only pre-release tags
- **WHEN** a repository has only the tag `ID@1.0.0-beta.1`
- **THEN** `install` fails with `no_releases` and `install SOURCE --ref ID@1.0.0-beta.1` installs it

#### Scenario: A consumer's range names a pre-release
- **WHEN** a consumer requires `lib ^1.3.0-beta.1`
- **THEN** a fresh install of `lib` for it takes `1.3.0-beta.1` (`dependency_ranges_pick_pre_releases_only_when_named`); `update all` never moves an installed release of `lib` onto a pre-release

#### Scenario: Following a beta
- **WHEN** `1.3.0-beta.1` is installed with `install-catalog ID --version 1.3.0-beta.1` and the origin has `1.3.0-beta.2` and `1.4.0-beta.1`, later also `1.3.0`
- **THEN** `update` moves to `1.3.0-beta.2`, then to `1.3.0`, and never to `1.4.0-beta.1`

## MODIFIED Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the package's origin and move to the newest release tag by SemVer precedence, also across a major version ("Major updates are marked"), passing over pre-releases ("Pre-releases are opt-in"). When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (same version on another commit: `ID updated V (old → new)` with short commits). A failing update or rollback leaves the workspace unchanged (errors, "Failing operations leave the workspace unchanged"). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update acme/greeter` twice
- **THEN** the first prints `acme/greeter updated 0.1.0 → 0.2.0 (major)`, the second `acme/greeter up to date (0.2.0)`

### Requirement: Joint upgrade of all packages
`update all` (`Manager::update_all`) SHALL resolve all installed packages together with PubGrub over the candidates of each package's mode (pre-releases only as in "Pre-releases are opt-in"), the installed revision as fallback and missing dependencies from the configured catalog (installed and recorded as in dependencies); it SHALL prefer newer versions with installed packages prioritized in lexical ID order, and apply the whole result as one recoverable transaction. Each update entry carries `major` ("Major updates are marked"). Verified by `update_all_coordinates_versions_and_adds_dependencies_atomically`.

#### Scenario: App and dependency cross a major boundary together
- **WHEN** app 2.0 requires lib ^2 and lib 2.0 exists, while app 1.x is installed with lib 1.x
- **THEN** `update all` moves both in one transaction and marks both lines `(major)`

### Requirement: Updates need no catalog
`update ID` (in any mode) and `update all` SHALL read releases and branches from the origin of the package's clone (set from `packages.<id>.source`) and SHALL NOT read a catalog for unprotected packages, so they work without one (protected packages: "Protected packages update through their catalog"); `check` reads the same origin for packages the catalog does not list (catalog). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `acme/greeter` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update acme/greeter` prints `acme/greeter updated 0.1.0 → 0.2.0 (major)`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `acme/greeter would be updated 0.1.0 → 0.2.0 (major)` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `acme/greeter` says `update available` with `0.2.0 (major)` from the origin, and nothing in the workspace changes

### Requirement: update follows the recorded mode
`update ID` without a target SHALL follow the package's recorded mode: release mode moves to the newest release, branch mode to the branch HEAD, and a pinned package SHALL stay unchanged with reason `pinned` (CLI: `ID pinned at V (REF)`). `--ref REF` and `--dev` SHALL move the package and record the new mode as at install (install-uninstall); `--release` SHALL return it to release mode at the newest release. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `acme/greeter` was installed with `--ref acme/greeter@0.1.0`, 0.2.0 is released and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)` and nothing changes

#### Scenario: Back to releases
- **WHEN** the user then runs `update acme/greeter --release`
- **THEN** it prints `acme/greeter updated 0.1.0 → 0.2.0 (major)` and the record has no `mode`

### Requirement: Protected packages update through their catalog
A protected package (recorded `catalog`) SHALL update only to versions its verified catalog lists, read from the recorded location whatever `Options.catalog` is; the release commit, package folder, manifest and file hashes SHALL match the record before any change (`catalog` otherwise). `update ID --detach` (`Manager::update_with`) SHALL drop the protection and update from the origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `acme/greeter` 0.2.0 has the tag `acme/greeter@0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update acme/greeter` prints `acme/greeter kept at 0.2.0 (upstream 0.3.0 not in catalog)` and `update all` lists it in `unchanged` with that reason
- **AND** a ref or branch target needs a detach: `update acme/greeter --ref acme/greeter@0.3.0` fails with `invalid_argument` ("… detach it to follow a ref or branch of its origin")

#### Scenario: Hash mismatch on update
- **WHEN** the verified catalog lists 0.3.0 but the files at the recorded commit do not match its hashes
- **THEN** `update acme/greeter` and `update all` fail with `catalog` ("catalog file hashes do not match git dist of acme/greeter@0.3.0: the release differs from the one added to the catalog") and the workspace is unchanged; a release commit the origin does not have fails with `catalog` ("… is not on the origin")

#### Scenario: Catalog no longer verified or unreachable
- **WHEN** the owner removed `"verified": true` from the catalog that protects `acme/greeter`, or the catalog cannot be read
- **THEN** `update acme/greeter` fails with `catalog` ("… is no longer verified; detach …") or `http` and nothing changes
- **AND** `update all` does not fail: it keeps `acme/greeter` in `unchanged` with the reason `catalog_unavailable` (`acme/greeter kept at 0.2.0 (catalog unavailable: REASON)`) and updates the other packages
- **AND** this is covered by `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`

#### Scenario: New dependency of a protected package
- **WHEN** protected `acme/app` 1.1.0 in its verified catalog adds a dependency `acme/lib` that is not installed, and no catalog or an unverified one is configured
- **THEN** `update acme/app` and `update all` install `acme/lib` from the recorded verified catalog and record it protected by that catalog
- **AND** this is covered by `workspace::new_dependencies_of_a_protected_package_come_from_its_catalog`

#### Scenario: Detach
- **WHEN** the user runs `update acme/greeter --detach`
- **THEN** it prints `acme/greeter detached from catalog LOCATION; it now follows its origin` and `acme/greeter updated 0.2.0 → 0.3.0 (major)`, the record loses `catalog` and later updates follow the origin
- **AND** a detach with no newer release is still a change (`workspace::detach_without_a_new_release_is_a_change`); detaching an unprotected package fails with `invalid_argument`; `update all --detach` is a usage error (exit status 1)
