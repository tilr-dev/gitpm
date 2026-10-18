## MODIFIED Requirements

### Requirement: Version format
Every package version (manifest, `forkOf`, `replaces`, `--version`, catalog, release tag) MUST be SemVer `MAJOR.MINOR.PATCH` with an optional pre-release `-PRE`, numbers without leading zeros; build metadata (`+…`) and anything else SHALL be refused with `invalid_version` (an empty `version` or `forkOf.version` in a manifest: `invalid_manifest`). Versions SHALL be ordered by SemVer precedence. Verified by `catalog::version_key_follows_semver_precedence`.

#### Scenario: Numeric ordering
- **WHEN** releases `1.9.0` and `1.10.0` exist
- **THEN** `1.10.0` is the newer release

#### Scenario: Pre-release suffix is refused
- **WHEN** a manifest declares version `01.0.0`, `1.0` or `1.0.0+build.5`
- **THEN** reading it fails with `invalid_version` (an empty `version` fails with `invalid_manifest`), while `1.0.0-beta.1` is accepted

#### Scenario: Pre-release ordering
- **WHEN** `1.0.0-alpha`, `1.0.0-beta`, `1.0.0-beta.2` and `1.0.0` exist
- **THEN** they are ordered `1.0.0-alpha < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0`
