## MODIFIED Requirements

### Requirement: Fork output names the next steps
The CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits and release all repository packages with one tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created
- **THEN** the output contains `Next steps (choose the packages to install and replace):` followed by the commit, install and replace commands

### Requirement: Explicit fork release preserves upstream tags
A fork SHALL retain inherited upstream tags. Releasing the fork with `commit DIR NEW_VERSION` SHALL create one repository release tag only for a new version. An existing `vVERSION` tag SHALL cause `version_conflict` without replacing that tag or installing any fork package.

#### Scenario: Fork retains upstream release tags
- **WHEN** a fork retains upstream `v1.0.0` and the user runs `gitpm commit FORK 1.0.0`
- **THEN** the command fails with `version_conflict` naming `v1.0.0`, and that upstream tag remains unchanged
- **AND** `gitpm commit FORK 1.1.0` can release every fork package at 1.1.0 when `v1.1.0` does not already exist

### Requirement: check shows fork upstream
For a fork, `check` SHALL add `upstream {id, base, latest, newer, source, problem}`: the original's latest release against the version the fork started from, taken from the catalog when it lists the original, otherwise from the release tags of `forkOf.source` (`source` `catalog` or `origin`); without either, `latest` is unknown. Verified by `check_shows_fork_upstream_and_other_catalogs` and `check::fork_upstream_from_fork_of_source`.

#### Scenario: Original has a newer release
- **WHEN** `me/hello` forked `acme/hello` 1.0.0 and the catalog has `acme/hello` 1.1.0
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base`

#### Scenario: Upstream without a catalog
- **WHEN** no catalog is given and `forkOf.source` of `me/hello` has the tag `v1.1.0`
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original at origin: 1.1.0 — newer than the fork base`

#### Scenario: Unreadable upstream
- **WHEN** no catalog lists the original and `forkOf.source` cannot be read
- **THEN** `upstream.problem` says why and `check` prints `me/hello: fork of acme/hello (base 1.0.0); original: unreachable (PROBLEM)`
