## MODIFIED Requirements

### Requirement: Fork output names the next steps
Without `--replace`, the CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits and release all repository packages with one tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created without `--replace`
- **THEN** the output contains `Next steps (or all at once: add --replace --version NEW_VERSION):` followed by the commit, install and replace commands

### Requirement: Fork and replace in one step
`fork … --replace [--version V] [--force]` SHALL release the forked folders under the rules of `commit` (authoring): one commit (`version` set for every package by `--version`, or the current shared version when omitted) and one repository tag (`vVERSION`, package-ids), shared by all packages at that version, then install each fork (dependencies first) and replace its original.

#### Scenario: Fork and replace in one step contract
- **WHEN** the user releases a fork with `fork … --replace`
- **THEN** Existing tags retained from upstream SHALL never be moved or removed to make a release succeed; when the requested version already has a tag, the author MUST choose a new `--version`. A fork with different current package versions SHALL require `--version` to release all packages at one shared version. If the release fails nothing SHALL be installed; a failed install or replace of one package SHALL NOT stop the others. Verified by `fork_replace_releases_installs_and_replaces_in_one_step`.

#### Scenario: Release without file transport
- **WHEN** a multi-package fork is released while Git's file transport is disabled
- **THEN** the single repository release tag is created at the release commit without using a transport
- **AND** verified by `fork_replace_release_without_file_transport`

#### Scenario: One-step fork
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into f --replace --version 1.1.0`, where `v1.1.0` does not already exist
- **THEN** `v1.1.0` is created for the repository, and `me/hello` is installed and replaces `acme/hello`, so only `dist/me/hello/` is deployed

#### Scenario: Failed release commit
- **WHEN** a pre-commit hook rejects the release commit of `fork … --replace`
- **THEN** the forked manifests and the index are as before the release, no new release tag exists (retained upstream tags are unchanged), nothing is installed, and the result reports the failed `release` step
- **AND** a wrong-typed metadata key fails the release step with `invalid_manifest` (plugin-metadata)
- **AND** verified by `fork_replace_release_rejecting_hook_restores_manifests_and_index` and `fork_replace_release_rejects_wrong_metadata_type`

#### Scenario: Fork retains upstream release tags
- **WHEN** a fork clone retains upstream `vVERSION` tags whose commits still name the original packages
- **THEN** those tags alone are not releases of the renamed fork packages, remain unchanged, and do not prevent a release with a distinct new version
- **AND** requesting a version whose repository tag already exists fails with `version_conflict` rather than replacing that upstream tag



#### Scenario: Default version collides with retained upstream tag
- **WHEN** the fork retained upstream `v1.0.0`, its current package version is 1.0.0, and the user runs `fork … --replace` without `--version`
- **THEN** the release step fails with `version_conflict` naming `v1.0.0`, no package is installed or replaced, and the upstream tag remains unchanged
- **AND** retrying with `--version 1.1.0` can release all fork packages at 1.1.0 when `v1.1.0` does not already exist

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
