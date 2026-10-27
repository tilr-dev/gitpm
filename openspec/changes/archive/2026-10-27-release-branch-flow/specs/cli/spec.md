## MODIFIED Requirements

### Requirement: Version string
`gitpm --version` (or `-V`) SHALL print `gitpm VERSION`; a build from a Git checkout of this repository (one that tracks `crates/gitpm-cli/build.rs`) whose HEAD is not at a tag matching `v[0-9]*` SHALL print `gitpm VERSION-dev+SHORTHASH`. Packaged builds (crates.io, release binaries) SHALL print the plain version. Verified by `version_tests::version_starts_with_the_package_version` and the CI smoke test.

#### Scenario: Dev build
- **WHEN** the binary is built from `main` at commit `0b146cf`
- **THEN** `gitpm --version` prints `gitpm 0.5.0-dev+0b146cf`
- **AND** `0.5.0-dev+0b146cf` is a SemVer pre-release and sorts below `0.5.0` although it was built after it; that is intended: a dev build is not that release
