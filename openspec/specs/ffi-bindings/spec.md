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
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and expose every locked workspace operation (catalog reads, install, download, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, switch back, sync-dist, with their plans); free functions: `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`, `uninstallRefusal` (throws `UninstallPlan::refusal`); author and catalog writers are not exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PackageManager` method, or one of the free functions `list`, `check`, `readState`
- **THEN** the method works on the opened (locked) workspace, while the free functions read `state.json` without opening, locking or creating it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview or the links of one catalog level
- **THEN** `PackageManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions `browseCatalog`, `catalogRelease` and `catalogLinks` take an explicit location without opening a workspace

#### Scenario: Install and update targets
- **WHEN** a host calls `install(source, target, package, force)`, `download(source, target, package)` or `update(id, target, detach)`
- **THEN** `install` and `download` take the `InstallTarget` enum `Release`, `Ref(reference)` or `Dev`, and `update` the `Target` enum with the same cases plus `Recorded` (following the recorded mode); the former `Channel` enum is gone

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PackageManager` has `install` (optional `package`, `force`), `installFromCatalog(force)`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)` (returning `ForgetResult`), `restore`, `uninstallPlan`, `uninstall(confirmed, force)` (`confirmed` the lost-work items the user saw), `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `forkReplaceResume`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack(confirm, force)`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id` for every error about a package, `LocalChanges.items`, `HasDependents.dependents` and `lostWork`, `IncompatibleReplacement.consumers`, `NotDeployed.sync`); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items
- Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Undeployed packages from Swift
- **WHEN** `syncDist` leaves an active package undeployed
- **THEN** Swift receives `PmError.NotDeployed` with the message and the `sync` report (what was deployed and removed, and the skipped packages with their reasons), as the CLI prints it

#### Scenario: Update refusal from Swift
- **WHEN** a branch-mode `update` of `acme/greeter` is refused because the history diverged
- **THEN** Swift receives `PmError.VersionConflict` whose `id` is `acme/greeter` and whose message names it

#### Scenario: Manifest refusal from Swift
- **WHEN** an update of `acme/greeter` finds another package ID or rollback finds a mismatched manifest
- **THEN** Swift receives `PmError.InvalidManifest` with `id` `acme/greeter` and the complete library message

#### Scenario: Unscoped validation error from Swift
- **WHEN** manifest or version validation fails without a package operation
- **THEN** the error keeps its original message and its optional `id` is absent

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, dropping none: `InstalledPackage` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `skipped`), every dist report, `CatalogRelease`, `PackageSummary`, `DistSync` (`SkippedDist.cloneMissing`), `UpdateAllPlan` (`unchanged`, `skipped`), `ReplacementPlan` (`installedVersion`), `ForgetResult`, `UninstallPlan` (`dependencyConflict`) and fork results.

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
- **THEN** `InstalledPackage`, `PackageRecord`, `CheckRow`, `RevisionChange` and `InstallResult` have `mode` (`release`, `pinned(reference)`, `branch(name)`), `Revision.mode` is the mode of a `previous` revision, `InstallResult` has `requestedMode`, and `RevisionChange.unchangedReason` and `Unchanged.reason` are `pinned(reference)`, `diverged(branch)`, `branchGone(branch)` or (`updateAll` only) `brokenDependency(dependency, problem)`

#### Scenario: Pinned package from Swift
- **WHEN** a host lists a workspace where `acme/greeter` was installed with `--ref acme/greeter@0.1.0` and then calls `update(id, .recorded)`
- **THEN** the listed `InstalledPackage.mode` is pinned to `acme/greeter@0.1.0`, and the returned `RevisionChange` is unchanged with `unchangedReason` `pinned`

### Requirement: Kept-back reasons in records
Each `keptBack` entry of the FFI `UpdateAllPlan` SHALL carry the library's `HeldBack.reason` as the `HeldBackReason` enum (`constraints`, `missingDependency(dependency, requirement)`, `noLongerReplaces(original)`) plus its `message`, and each `unchanged` entry the `message` of its `reason`, so a host explains a kept-back or skipped package as `gitpm update all` does. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Broken consumer from Swift
- **WHEN** `lib` was force-uninstalled while `app` depends on it and a host calls `updateAllPlan`
- **THEN** the plan's `unchanged` has `app` with `reason` `brokenDependency(dependency: "acme/lib", problem: "not installed")` and `message` `dependency acme/lib is not installed`

#### Scenario: Fork kept back from Swift
- **WHEN** the newest release of a replacing fork no longer declares `replaces` and a host calls `updateAllPlan`
- **THEN** the fork's `keptBack` entry has `reason` `noLongerReplaces(original)` and `message` `no longer replaces ORIGINAL`

### Requirement: Tag warnings in records
The warnings of moved and retracted release tags SHALL reach the host: `RevisionChange.warnings` for `update ID`, and the `warnings` of each `UpdateAllPlan.unchanged` entry for `update all`. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Retracted release from Swift
- **WHEN** the origin deleted the tag of the installed release and a host calls `updateAll`
- **THEN** the package is in `unchanged` with the warning `installed release ID@V no longer exists on the origin`

### Requirement: Catalog verification in records
FFI records SHALL carry every field protected catalogs add: `CheckReport.catalogVerified`, the status `upstreamNotInCatalog` with `CheckRow.originLatest`, `CatalogOverview.verified`, `RevisionChange.detached`, `UnchangedReason.notInCatalog(version)` and `UnchangedReason.catalogUnavailable(reason)`; `update(id, target, detach)` SHALL take the detach flag (default `false`). Verified by `records_convert_field_by_field`.

#### Scenario: Verified catalog from Swift
- **WHEN** a host calls `check(root, location)` with a catalog whose `catalog.json` has `"verified": true`
- **THEN** the report has `catalogVerified` true, and a protected package whose origin has a release the catalog lacks has status `upstreamNotInCatalog`, `originLatest` that version and `statusMessage` `upstream V not in catalog`

#### Scenario: Detach from Swift
- **WHEN** a host calls `update(id, .recorded, detach: true)` on a protected package
- **THEN** the returned `RevisionChange.detached` names the catalog location, and a later `update(id, .recorded)` without a detach on a protected package that stays returns `unchangedReason` `notInCatalog(version)` when the origin is ahead of the catalog

### Requirement: Major and pre-release flags in records
FFI records SHALL carry the version flags of the library results: `RevisionChange.major`, `major` on each `UpdateAllPlan` update entry, and `major` and `prerelease` on each `CheckReport` row. This adds to "Records mirror library results" and "Check report record". Verified by `records_convert_field_by_field`.

#### Scenario: Major update from Swift
- **WHEN** a host updates `acme/greeter` from 1.2.3 to 2.0.0
- **THEN** the returned `RevisionChange` has `major` true, as the CLI line ends with ` (major)`

#### Scenario: Pre-release in a check row
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the host's `CheckReport` row has `major` true and `prerelease` `2.1.0-rc.1`
