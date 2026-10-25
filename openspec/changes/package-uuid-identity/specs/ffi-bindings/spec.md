## MODIFIED Requirements

### Requirement: Manager surface
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and expose every locked workspace operation (catalog reads, install, download, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, replacement check/resolve, sync-dist, with their plans); free functions: `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`, `uninstallRefusal` (throws `UninstallPlan::refusal`).

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
- **THEN** `PackageManager` has `install` (optional `package`, `force`), `installFromCatalog(force)`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)` (returning `ForgetResult`), `restore`, `uninstallPlan`, `uninstall(confirmed, force)` (`confirmed` the lost-work items the user saw), `fork`, `forkPackages`, `replacePlan`, `replaceCheck`, `replaceResolve`, `replace` (without confirmation), `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog update`, `catalog register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

#### Scenario: Fork is a separate operation
- **WHEN** a host uses the fork API
- **THEN** `fork` and `forkPackages` prepare the editable repository without releasing, installing or replacing packages
- **AND** composed fork/release/install/replace and fork resume methods are not exported

#### Scenario: Explicit consistency methods
- **WHEN** a host calls replaceCheck(original, replacement)
- **THEN** it receives the same dependency issues and missing installs as the library, without changing the workspace; replaceResolve explicitly installs missing dependencies without applying the binding

#### Scenario: UUID identity crosses the boundary
- **WHEN** the host selects, forks or replaces a package through FFI
- **THEN** fork takes no namespace and returns fresh UUIDs, and all package references contain UUIDs without name hints

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
- **WHEN** a branch-mode `update` of `d135a7cf-1c63-549a-93cb-8f89656b2ced` is refused because the history diverged
- **THEN** Swift receives `PmError.VersionConflict` whose `id` is `d135a7cf-1c63-549a-93cb-8f89656b2ced` and whose message names it

#### Scenario: Manifest refusal from Swift
- **WHEN** an update of `d135a7cf-1c63-549a-93cb-8f89656b2ced` finds another package ID or rollback finds a mismatched manifest
- **THEN** Swift receives `PmError.InvalidManifest` with `id` `d135a7cf-1c63-549a-93cb-8f89656b2ced` and the complete library message

#### Scenario: Unscoped validation error from Swift
- **WHEN** manifest or version validation fails without a package operation
- **THEN** the error keeps its original message and its optional `id` is absent

### Requirement: Records carry the recorded mode
FFI records SHALL carry every field the recorded modes add to library results, as the `Mode` and `UnchangedReason` enums. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Mode fields
- **WHEN** Swift bindings are generated
- **THEN** `InstalledPackage`, `PackageRecord`, `CheckRow`, `RevisionChange` and `InstallResult` have `mode` (`release`, `pinned(reference)`, `branch(name)`), `Revision.mode` is the mode of a `previous` revision, `InstallResult` has `requestedMode`, and `RevisionChange.unchangedReason` and `Unchanged.reason` are `pinned(reference)`, `diverged(branch)`, `branchGone(branch)` or (`updateAll` only) `brokenDependency(dependency, problem)`

#### Scenario: Pinned package from Swift
- **WHEN** a host lists a workspace where `d135a7cf-1c63-549a-93cb-8f89656b2ced` was installed with `--ref v0.1.0` and then calls `update(id, .recorded)`
- **THEN** the listed `InstalledPackage.mode` is pinned to `v0.1.0`, and the returned `RevisionChange` is unchanged with `unchangedReason` `pinned`

### Requirement: Kept-back reasons in records
Each `keptBack` entry of the FFI `UpdateAllPlan` SHALL carry the library's `HeldBack.reason` as the `HeldBackReason` enum (`constraints`, `missingDependency(dependency, requirement)`, `noLongerReplaces(original)`) plus its `message`, and each `unchanged` entry the `message` of its `reason`, so a host explains a kept-back or skipped package as `gitpm update all` does. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Broken consumer from Swift
- **WHEN** `lib` was force-uninstalled while `app` depends on it and a host calls `updateAllPlan`
- **THEN** the plan's `unchanged` has `app` with `reason` `brokenDependency(dependency: "cfbc4d17-8707-5571-853d-c9136a7f9f21", problem: "not installed")` and `message` `dependency cfbc4d17-8707-5571-853d-c9136a7f9f21 is not installed`

#### Scenario: Fork kept back from Swift
- **WHEN** the newest release of a replacing fork no longer declares `replaces` and a host calls `updateAllPlan`
- **THEN** the fork's `keptBack` entry has `reason` `noLongerReplaces(original)` and `message` `no longer replaces ORIGINAL`

### Requirement: Major and pre-release flags in records
FFI records SHALL carry the version flags of the library results: `RevisionChange.major`, `major` on each `UpdateAllPlan` update entry, and `major` and `prerelease` on each `CheckReport` row. This adds to "Records mirror library results" and "Check report record". Verified by `records_convert_field_by_field`.

#### Scenario: Major update from Swift
- **WHEN** a host updates `d135a7cf-1c63-549a-93cb-8f89656b2ced` from 1.2.3 to 2.0.0
- **THEN** the returned `RevisionChange` has `major` true, as the CLI line ends with ` (major)`

#### Scenario: Pre-release in a check row
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the host's `CheckReport` row has `major` true and `prerelease` `2.1.0-rc.1`
