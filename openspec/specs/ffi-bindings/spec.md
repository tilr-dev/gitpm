# ffi-bindings Specification

## Purpose

Defines the UniFFI wrapper `gitpm-ffi` that exposes the library to native hosts (Swift module `Gitpm`, C module `GitpmFFI`, Kotlin package `gitpm`): its surface, error mapping, record conversion and how it is built and distributed.

## Requirements

### Requirement: Binding names
The FFI crate SHALL generate the Swift module `Gitpm` over the C module `GitpmFFI` and the Kotlin package `gitpm` (names in `crates/gitpm-ffi/uniffi.toml`, UniFFI 0.32, proc-macros without UDL), built as `staticlib` and `lib`.

#### Scenario: Swift import
- **WHEN** a Swift host links the static library and the generated `Gitpm.swift`
- **THEN** it can `import Gitpm` and call `PluginManager.open(root:options:)`

### Requirement: Manager surface
`PluginManager` SHALL be opened with `open(root, ManagerOptions)` and SHALL expose every workspace operation of the library (list, check, catalog reads, install, update and update all, rollback, enable/disable, forget/restore, uninstall plan and uninstall, fork variants, replace and switch back with plans, sync-dist with plan), plus free functions `authCheck`, `browseCatalog`, `catalogRelease`, `readInstalled` and `describeLostWork`. Author and catalog-generation commands SHALL NOT be exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PluginManager` method, `check` included, or `readInstalled`
- **THEN** the method works on the opened (locked) workspace, while `readInstalled` reads `state.json` without opening it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview
- **THEN** `PluginManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions of the same names take an explicit location without opening a workspace

#### Scenario: Update channel
- **WHEN** a host calls `update(id, reference, channel)`
- **THEN** `channel` is the `Channel` enum `Stable` or `Dev`

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PluginManager` has `install` (optional `package`), `installFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable`, `disable(force)`, `forget(force)`, `restore`, `uninstallPlan`, `uninstall(confirm, force)`, `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack`, `syncDistPlan` and `syncDist`

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id`, `LocalChanges.items`, `HasDependents.dependents`, `IncompatibleReplacement.consumers`); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later. Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, including `InstalledPlugin` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`), `CatalogRelease` (`path`, metadata), `PluginSummary`, `DistSync` (deployed, removed, unmanaged, skipped), `UpdateAllPlan`, and fork results with `ForkStep` (release, install, replace), `errorKind` and `errorMessage`.

#### Scenario: Partial fork failure
- **WHEN** `forkAndReplace` fails at the install step
- **THEN** the result has `failedStep` `install` with the error kind and message, not a thrown error
- **AND** this is covered by `records_convert_field_by_field` and `fork_replace_result_reports_the_failed_step_with_kind_and_message`

### Requirement: FFI is not published
`gitpm-ffi` SHALL stay `publish = false`, keep its own version and SHALL NOT be checked on the MSRV gate. Apple artifacts SHALL be built by `scripts/build-apple.sh` (macOS staticlib, `Gitpm.swift`, headers, `GitpmFFI.xcframework` only when `xcodebuild` exists), which skips and reports missing rustup targets, never installs toolchains and fails when no Apple target is installed.

#### Scenario: One target installed
- **WHEN** only `aarch64-apple-darwin` is installed
- **THEN** the library is that target's staticlib, not a universal one; `lipo` merges only when both Apple targets were built

#### Scenario: Missing target
- **WHEN** `x86_64-apple-darwin` is not installed and `scripts/build-apple.sh` runs
- **THEN** the script reports the skipped target and builds the rest
