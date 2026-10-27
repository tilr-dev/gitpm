## MODIFIED Requirements

### Requirement: Version format
Every package version (manifest `version`, `forkOf.version`, `replaces.version`, `--version` arguments, catalog versions, release tags) MUST be SemVer `MAJOR.MINOR.PATCH` of decimal numbers without leading zeros, with an optional pre-release `-PRE` (dot-separated identifiers of ASCII letters, digits and hyphens; numeric identifiers without leading zeros); anything else, build metadata (`+…`) included, SHALL be refused with `invalid_version`, except that an empty `version` or `forkOf.version` inside a manifest is refused with `invalid_manifest`. Versions SHALL be ordered by SemVer precedence: numerically, and a pre-release before its release. Verified by `catalog::version_key_follows_semver_precedence` and `catalog::version_key_rejects_everything_else`.

#### Scenario: Numeric ordering
- **WHEN** releases `1.9.0` and `1.10.0` exist
- **THEN** `1.10.0` is the newer release

#### Scenario: Pre-release suffix is refused
- **WHEN** a manifest declares version `01.0.0`, `1.0`, a malformed pre-release suffix `1.0.0-01` or the build suffix `1.0.0+build.5`
- **THEN** reading it fails with `invalid_version` (an empty `version` fails with `invalid_manifest`), while the pre-release `1.0.0-beta.1` is accepted

#### Scenario: Pre-release ordering
- **WHEN** `1.0.0-alpha`, `1.0.0-beta`, `1.0.0-beta.2` and `1.0.0` exist
- **THEN** they are ordered `1.0.0-alpha < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0`

### Requirement: Release tag names
Single source for tag names: the release tag of a package SHALL be `namespace/name@VERSION` with VERSION as in "Version format" (a pre-release included), both for tags gitpm creates (`init`, `commit`, `fork --replace`) and for tags it reads, in single- and multi-package repositories. Any other tag, `vVERSION` and versions with build metadata included, SHALL NOT be a release tag. A tag SHALL count only when the manifest at its commit has that ID and version. Verified by `identity::release_tag_is_id_at_version`.

#### Scenario: Tag disagrees with the manifest
- **WHEN** `acme/hello` at version 1.0.0 gets the newest tag `acme/hello@8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag acme/hello@8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog-update` fails with `version_conflict` and writes nothing

#### Scenario: Two tags for one version
- **WHEN** `v1.0.0` and `dan/demo1@1.0.0` point to different commits
- **THEN** only `dan/demo1@1.0.0` is a release of version 1.0.0 and discovery reports no conflict

#### Scenario: Scoped tag
- **WHEN** `dan/demo1` is released at `1.2.3`
- **THEN** the release tag is `dan/demo1@1.2.3`

#### Scenario: Legacy tag
- **WHEN** `commit` releases `dan/demo1` at `1.2.3`
- **THEN** it creates `dan/demo1@1.2.3` and never a `v1.2.3` tag

#### Scenario: vVERSION tag of a scoped single-package repository
- **WHEN** a single-package repository whose manifest ID is `dan/demo1` has only the tag `v1.0.0` on a commit with version `1.0.0`
- **THEN** `install` fails with `no_releases` and the workspace is unchanged

#### Scenario: Pre-release tag
- **WHEN** a repository has the tag `dan/demo1@2.0.0-beta.1` on a commit whose manifest has version `2.0.0-beta.1`
- **THEN** it is a release tag of version `2.0.0-beta.1`; install, update and `update all` pick it only when asked for (updates, "Pre-releases are opt-in"), and `catalog-update` publishes it

#### Scenario: Tags that are not release versions
- **WHEN** a repository has tags `dan/demo1@2.0.0+b1` and `dan/demo1@latest` besides valid releases
- **THEN** those tags are not releases: install, update and `catalog-update` never select them and do not warn about them
