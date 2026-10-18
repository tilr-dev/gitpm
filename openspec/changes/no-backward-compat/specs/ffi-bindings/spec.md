## MODIFIED Requirements

### Requirement: FFI is not published
`gitpm-ffi` SHALL stay `publish = false`, carry the library's version (release-process, "Lockstep crates") and SHALL NOT be checked on the MSRV gate; before 1.0 its bindings SHALL promise no stability. Apple artifacts SHALL be built by `scripts/build-apple.sh` (macOS staticlib, `Gitpm.swift`, headers, `GitpmFFI.xcframework` only when `xcodebuild` exists), which skips and reports missing rustup targets, never installs toolchains and fails when no Apple target is installed.

#### Scenario: Breaking binding change
- **WHEN** a release renames or removes an FFI method or record field
- **THEN** README "Changed in X.Y" marks it **BREAKING**; no deprecated alias is kept

#### Scenario: One target installed
- **WHEN** only `aarch64-apple-darwin` is installed
- **THEN** the library is that target's staticlib, not a universal one; `lipo` merges only when both Apple targets were built

#### Scenario: Missing target
- **WHEN** `x86_64-apple-darwin` is not installed and `scripts/build-apple.sh` runs
- **THEN** the script reports the skipped target and builds the rest
