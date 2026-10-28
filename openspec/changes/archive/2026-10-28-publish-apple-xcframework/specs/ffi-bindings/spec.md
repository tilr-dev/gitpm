## MODIFIED Requirements

### Requirement: FFI is not published
`gitpm-ffi` SHALL stay `publish = false`, carry the library's version ("Lockstep crates"), skip the MSRV gate and promise no binding stability before 1.0. Apple artifacts SHALL use `scripts/build-apple.sh`, never installing toolchains. Default builds SHALL report and skip missing targets and optionally create an xcframework; `--release` MUST require both Apple targets and xcodebuild, use locked dependencies and fail on missing prerequisites or build errors. Verified by release workflow tests.

#### Scenario: Breaking binding change
- **WHEN** a release renames or removes an FFI method or record field
- **THEN** README "Changed in X.Y" marks it **BREAKING**; no deprecated alias is kept

#### Scenario: One target installed
- **WHEN** only `aarch64-apple-darwin` is installed and a default local build runs
- **THEN** the library is that target's staticlib, not a universal one; `lipo` merges only when both Apple targets were built

#### Scenario: Missing target
- **WHEN** `x86_64-apple-darwin` is not installed and a default local build runs
- **THEN** the script reports the skipped target and builds the rest

#### Scenario: No Apple targets
- **WHEN** neither Apple target is installed
- **THEN** the build fails

#### Scenario: Local artifacts
- **WHEN** a default local build succeeds
- **THEN** it produces the macOS staticlib, `swift/Gitpm.swift` and headers
- **AND** it produces `GitpmFFI.xcframework` only when xcodebuild exists

#### Scenario: Release prerequisites
- **WHEN** a release build lacks `aarch64-apple-darwin`, `x86_64-apple-darwin` or `xcodebuild`
- **THEN** the script fails rather than skipping the missing prerequisite or producing a partial artifact

#### Scenario: Universal release build
- **WHEN** a release build has both targets and `xcodebuild`
- **THEN** it builds both architectures with the locked dependencies and produces the universal xcframework and matching Swift bindings
