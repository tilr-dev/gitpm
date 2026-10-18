## ADDED Requirements

### Requirement: Catalog versions follow SemVer and show pre-releases
Catalog versions SHALL be ordered by SemVer precedence (package-ids, "Version format"); pre-releases are listed by `info` and installable with `--version`. The latest version of a package (`PackageSummary.latest`, an install without `--version`, what `check` compares with) SHALL be its newest version that is not a pre-release (updates, "Pre-releases are opt-in"). A `check` row SHALL carry `major` and `prerelease`. Verified by `catalog::latest_skips_pre_releases_and_check_marks_major`.

#### Scenario: Catalog with a beta
- **WHEN** the catalog has `acme/greeter` 1.2.0 and 1.3.0-beta.1 and the user runs `install-catalog acme/greeter`
- **THEN** 1.2.0 is installed; `install-catalog acme/greeter --version 1.3.0-beta.1` installs the beta in release mode, and `update` then follows it to newer pre-releases of 1.3.0 and to 1.3.0

#### Scenario: check output
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the row says `update available; pre-release 2.1.0-rc.1` with LATEST `2.0.0 (major)`; the row carries `major` true and `prerelease` `2.1.0-rc.1`

#### Scenario: Pre-releases only
- **WHEN** the catalog lists `acme/beta` only at 1.0.0-beta.1
- **THEN** `info` shows `latest: pre-releases only`, `install-catalog acme/beta` fails with `catalog` and `install-catalog acme/beta --version 1.0.0-beta.1` installs it

#### Scenario: Origin rows
- **WHEN** a package without a catalog is at 1.1.0 and its origin has the tags `ID@2.0.0-beta.1` and later `ID@2.0.0`
- **THEN** `check` first shows `up to date; pre-release 2.0.0-beta.1`, then LATEST `2.0.0 (major)` with `update available`
