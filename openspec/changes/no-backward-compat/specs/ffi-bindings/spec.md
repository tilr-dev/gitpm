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

### Requirement: Binding names
The FFI crate SHALL generate the Swift module `Gitpm` over the C module `GitpmFFI` and the Kotlin package `gitpm` (names in `crates/gitpm-ffi/uniffi.toml`, UniFFI 0.32, proc-macros without UDL), built as `staticlib` and `lib`. Exported objects and records SHALL say "package", never "plugin": `PackageManager`, `InstalledPackage`, `PackageSummary`. Verified by `manager_round_trip_on_empty_workspace` and the Swift generation of `scripts/build-apple.sh`.

#### Scenario: Swift import
- **WHEN** a Swift host links the static library and the generated `Gitpm.swift`
- **THEN** it can `import Gitpm` and call `PackageManager.open(root:options:)`

#### Scenario: Former names
- **WHEN** a host compiled against 0.5 calls `PluginManager` or uses `InstalledPlugin`
- **THEN** it no longer compiles; README "Changed in 0.6" lists the renames as **BREAKING** and no alias is kept
