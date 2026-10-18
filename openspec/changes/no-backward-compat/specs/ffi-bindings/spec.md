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

### Requirement: Manager surface
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and SHALL expose every workspace operation of the library (list, check, catalog reads, install, update and update all, rollback, enable/disable, forget/restore, uninstall plan and uninstall, fork variants, replace and switch back with plans, sync-dist with plan), plus free functions `authCheck`, `browseCatalog`, `catalogRelease`, `readInstalled` and `describeLostWork`. Author and catalog-generation commands SHALL NOT be exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PackageManager` method, `check` included, or `readInstalled`
- **THEN** the method works on the opened (locked) workspace, while `readInstalled` reads `state.json` without opening it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview
- **THEN** `PackageManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions of the same names take an explicit location without opening a workspace

#### Scenario: Update channel
- **WHEN** a host calls `update(id, reference, channel)`
- **THEN** `channel` is the `Channel` enum `Stable` or `Dev`

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PackageManager` has `install` (optional `package`), `installFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable`, `disable(force)`, `forget(force)`, `restore`, `uninstallPlan`, `uninstall(confirm, force)`, `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack`, `syncDistPlan` and `syncDist`

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, including `InstalledPackage` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`), `CatalogRelease` (`path`, metadata), `PackageSummary`, `DistSync` (deployed, removed, unmanaged, skipped), `UpdateAllPlan`, and fork results with `ForkStep` (release, install, replace), `errorKind` and `errorMessage`.

#### Scenario: Partial fork failure
- **WHEN** `forkAndReplace` fails at the install step
- **THEN** the result has `failedStep` `install` with the error kind and message, not a thrown error
- **AND** this is covered by `records_convert_field_by_field` and `fork_replace_result_reports_the_failed_step_with_kind_and_message`
