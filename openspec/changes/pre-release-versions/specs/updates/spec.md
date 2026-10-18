## ADDED Requirements

### Requirement: Major updates are marked
An update that crosses a breaking boundary (a new MAJOR, or for 0.x a new MINOR) SHALL be marked `major` in `RevisionChange` and in each `update all` entry, and the CLI SHALL append ` (major)` to its line. The author marks a breaking release only by its version number. Dependents stay protected by their own requirements. Verified by `workspace::major_updates_are_marked`.

#### Scenario: Major line
- **WHEN** `acme/greeter` 1.2.3 is installed, 2.0.0 is released and nothing depends on it, and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter updated 1.2.3 → 2.0.0 (major)`

#### Scenario: 0.x minor
- **WHEN** a package moves from 0.2.5 to 0.3.0
- **THEN** the change is marked `(major)`

### Requirement: Pre-releases are opt-in
Install, update and `update all` SHALL never pick a pre-release implicitly. A pre-release SHALL be installed only when named explicitly (`--ref ID@1.3.0-beta.1`, `install-catalog ID --version 1.3.0-beta.1`); once one is installed, `update` SHALL also consider newer pre-releases of the same MAJOR.MINOR.PATCH and every release. Verified by `workspace::pre_releases_are_opt_in`.

#### Scenario: Newest tag is a pre-release
- **WHEN** a package is at 1.2.0 and the origin also has `1.3.0-beta.1`
- **THEN** `update` reports `up to date (1.2.0)`

#### Scenario: Following a beta
- **WHEN** `1.3.0-beta.1` is installed and the origin has `1.3.0-beta.2`, later `1.3.0`
- **THEN** `update` moves to `1.3.0-beta.2`, then to `1.3.0`, and never to `1.4.0-beta.1`

## MODIFIED Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the origin and move to the newest release by SemVer, also across a major version ("Major updates are marked", "Pre-releases are opt-in"). When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (branch mode, same version on another commit: `ID updated V (old → new)`). A failing update or rollback leaves the workspace unchanged (errors). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update acme/greeter` twice
- **THEN** the first prints `acme/greeter updated 0.1.0 → 0.2.0`, the second `acme/greeter up to date (0.2.0)`

### Requirement: Joint upgrade of all packages
`update all` (`Manager::update_all`) SHALL resolve all installed packages together with PubGrub over the release tags of each origin ("Pre-releases are opt-in"), the installed revision as fallback and missing dependencies from the configured catalog (as in dependencies); it SHALL prefer newer versions, installed packages first in lexical ID order, and apply the result as one recoverable transaction. Verified by `update_all_coordinates_versions_and_adds_dependencies_atomically`.

#### Scenario: App and dependency cross a major boundary together
- **WHEN** app 2.0 requires lib ^2 and lib 2.0 exists, while app 1.x is installed with lib 1.x
- **THEN** `update all` moves both in one transaction and marks both lines `(major)`

