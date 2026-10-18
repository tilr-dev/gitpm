## MODIFIED Requirements

### Requirement: Manager surface
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and expose every workspace operation that takes the lock (catalog reads, install, download, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, switch back, sync-dist, with their plans); free functions SHALL be `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`, `uninstallRefusal` (throws the `HasDependents` refusal of an `UninstallPlan`, as `UninstallPlan::refusal`). Author and catalog-writing commands are not exposed.

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
- **THEN** `PackageManager` has `install` (optional `package`, `force`), `installFromCatalog(force)`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)` (returning `ForgetResult`), `restore`, `uninstallPlan`, `uninstall(confirmed, force)` (`confirmed` the lost-work items the user saw), `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `forkReplaceResume`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack(confirm, force)`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id`, `LocalChanges.items`, `HasDependents.dependents` and `lostWork`, `IncompatibleReplacement.consumers`, `NotDeployed.sync`); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later. Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items

#### Scenario: Undeployed packages from Swift
- **WHEN** `syncDist` leaves an active package undeployed
- **THEN** Swift receives `PmError.NotDeployed` with the message and the `sync` report (what was deployed and removed, and the skipped packages with their reasons), as the CLI prints it

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, dropping none: `InstalledPackage` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `skipped`), the dist report of every mutating result, `CatalogRelease`, `PackageSummary`, `DistSync`, `UpdateAllPlan` (`unchanged`, `skipped`), `ReplacementPlan` (`installedVersion`), `ForgetResult` (`cloneKept`, `dist`), `UninstallPlan` (`dependencyConflict`), `SkippedDist` (`cloneMissing`), and fork results (scenario "Fork results").

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
