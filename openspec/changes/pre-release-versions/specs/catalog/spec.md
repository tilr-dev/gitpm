## ADDED Requirements

### Requirement: Catalog versions follow SemVer and show pre-releases
Catalog versions SHALL be ordered by SemVer precedence (package-ids, "Version format"). The latest version of a package for `install-catalog` without `--version` and for `check` SHALL be its newest release that is not a pre-release; pre-releases SHALL still be listed by `info`/`browse` and installable with `--version`. `check` SHALL mark an available major update `(major)` and name a newer pre-release without offering it. Verified by `catalog::latest_skips_pre_releases_and_check_marks_major`.

#### Scenario: Catalog with a beta
- **WHEN** the catalog has `acme/greeter` 1.2.0 and 1.3.0-beta.1 and the user runs `install-catalog acme/greeter`
- **THEN** 1.2.0 is installed; `install-catalog acme/greeter --version 1.3.0-beta.1` installs the beta

#### Scenario: check output
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the row says `update available` with `2.0.0 (major)` and notes `pre-release 2.1.0-rc.1`; the report carries `major` and `prerelease`
