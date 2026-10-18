# ffi-bindings Specification

## Purpose

Defines the UniFFI wrapper `gitpm-ffi` that exposes the library to native hosts (Swift module `Gitpm`, C module `GitpmFFI`, Kotlin package `gitpm`): its surface, error mapping, record conversion and how it is built and distributed.

## Requirements

### Requirement: Binding names
The FFI crate SHALL generate the Swift module `Gitpm` over the C module `GitpmFFI` and the Kotlin package `gitpm` (names in `crates/gitpm-ffi/uniffi.toml`, UniFFI 0.32, proc-macros without UDL), built as `staticlib` and `lib`. Exported objects and records SHALL say "package", never "plugin": `PackageManager`, `InstalledPackage`, `PackageSummary`. Verified by `manager_round_trip_on_empty_workspace` and the Swift generation of `scripts/build-apple.sh`.

#### Scenario: Swift import
- **WHEN** a Swift host links the static library and the generated `Gitpm.swift`
- **THEN** it can `import Gitpm` and call `PackageManager.open(root:options:)`

### Requirement: Manager surface
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and expose every workspace operation that takes the lock (catalog reads, install, download, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, switch back, sync-dist, with their plans); free functions SHALL be `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`. Author and catalog-writing commands are not exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PackageManager` method, or one of the free functions `list`, `check`, `readState`
- **THEN** the method works on the opened (locked) workspace, while the free functions read `state.json` without opening, locking or creating it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview or the links of one catalog level
- **THEN** `PackageManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions `browseCatalog`, `catalogRelease` and `catalogLinks` take an explicit location without opening a workspace

#### Scenario: Install and update targets
- **WHEN** a host calls `install(source, target, package, force)`, `download(source, target, package)` or `update(id, target)`
- **THEN** `install` and `download` take the `InstallTarget` enum `Release`, `Ref(reference)` or `Dev`, and `update` the `Target` enum with the same cases plus `Recorded` (following the recorded mode); the former `Channel` enum is gone

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PackageManager` has `install` (optional `package`, `force`), `installFromCatalog(force)`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)`, `restore`, `uninstallPlan`, `uninstall(confirm, force)`, `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `forkReplaceResume`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id`, `LocalChanges.items`, `HasDependents.dependents` and `.replacing`, `IncompatibleReplacement.consumers`, `NotDeployed.sync`); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later. Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items

#### Scenario: Undeployed packages from Swift
- **WHEN** `syncDist` leaves an active package undeployed
- **THEN** Swift receives `PmError.NotDeployed` with the message and the `sync` report (what was deployed and removed, and the skipped packages with their reasons), as the CLI prints it

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, dropping none: `InstalledPackage` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `skipped`), the dist report of every mutating result, `CatalogRelease`, `PackageSummary`, `DistSync`, `UpdateAllPlan` (`unchanged`, `skipped`), `ReplacementPlan` (`installedVersion`, `forkEnabled`), and fork results (scenario "Fork results").

#### Scenario: Fork results
- **WHEN** a host reads the result of `forkAndReplace`, `forkPackagesAndReplace` or `forkReplaceResume`
- **THEN** it gets `ForkStep`, `errorKind`, `errorMessage` and the pending plan per package, and `ForkResume` with `applied` and `failed`

#### Scenario: Partial fork failure
- **WHEN** `forkAndReplace` fails at the install step
- **THEN** the result has `failedStep` `install` with the error kind and message, not a thrown error
- **AND** this is covered by `records_convert_field_by_field` and `fork_replace_result_reports_the_failed_step_with_kind_and_message`

#### Scenario: Warnings reach the host
- **WHEN** an install skips a broken historical tag and ignores a wrong-typed metadata key
- **THEN** the FFI `InstallResult` carries both in `skipped` and `warnings`, as the CLI prints them; an update past such a tag carries it in `RevisionChange.skipped`, `update all` in `UpdateAllPlan.skipped`

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

### Requirement: CLI and FFI parity
Every CLI command that reads or changes a workspace, reads a catalog or checks access SHALL have an FFI equivalent that returns the data the CLI prints, so a host never re-implements CLI logic. The stated exceptions are the author commands (`init`, `commit`, `remote`) and the catalog writers (`catalog-update`, `register`), which stay library and CLI functions. Verified by `ffi_covers_every_workspace_and_catalog_read_command`.

#### Scenario: list with a filter
- **WHEN** a host needs what `gitpm list --provides theme` prints
- **THEN** it calls the free function `list(root, provides)` and gets the same state and `resolved` entries

#### Scenario: Restorable packages
- **WHEN** a host wants to offer `restore` for forgotten packages
- **THEN** `readState(root)` returns the full state view (raw state including `forgotten` and `replacements`, plus `resolved` with `active`) without opening the workspace; it is the only free function for the state

### Requirement: Check report record
The free function `check(root, catalog)` SHALL return a `CheckReport` record that mirrors the library report field by field: rows (with `status`, message, `source` `catalog`/`origin` and `problem`), the fork `upstream` of each row (with its `source` and `problem`), the replacement bindings and the unsatisfied dependencies, so a host shows what `gitpm check` prints. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: check without a catalog from Swift
- **WHEN** a host calls `check(root, nil)` on a workspace with packages installed from Git
- **THEN** it receives a `CheckReport` whose rows have `source` `origin`, plus the replacement bindings and unsatisfied dependencies the CLI prints

### Requirement: Records carry the recorded mode
FFI records SHALL carry every field the recorded modes add to library results, as the `Mode` and `UnchangedReason` enums. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Mode fields
- **WHEN** Swift bindings are generated
- **THEN** `InstalledPackage`, `PackageRecord`, `CheckRow`, `RevisionChange` and `InstallResult` have `mode` (`release`, `pinned(reference)`, `branch(name)`), `Revision.mode` is the mode of a `previous` revision, `InstallResult` has `requestedMode`, and `RevisionChange.unchangedReason` and `Unchanged.reason` are `pinned(reference)`, `diverged(branch)` or `branchGone(branch)`

#### Scenario: Pinned package from Swift
- **WHEN** a host lists a workspace where `acme/greeter` was installed with `--ref acme/greeter@0.1.0` and then calls `update(id, .recorded)`
- **THEN** the listed `InstalledPackage.mode` is pinned to `acme/greeter@0.1.0`, and the returned `RevisionChange` is unchanged with `unchangedReason` `pinned`
