## MODIFIED Requirements

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
- **THEN** `PackageManager` has `install` (optional `package`, `force`), `installFromCatalog(force)`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)` (returning `ForgetResult`), `restore`, `uninstallPlan`, `uninstall(confirmed, force)` (`confirmed` the lost-work items the user saw), `fork`, `forkPackages`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack(confirm, force)`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog update`, `catalog register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

#### Scenario: Fork is a separate operation
- **WHEN** a host uses the fork API
- **THEN** `fork` and `forkPackages` prepare the editable repository without releasing, installing or replacing packages
- **AND** composed fork/release/install/replace and fork resume methods are not exported

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field, dropping none: `InstalledPackage` (`active`, `clone`, `path`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `skipped`), every dist report, `CatalogRelease`, `PackageSummary`, `DistSync` (`SkippedDist.cloneMissing`), `UpdateAllPlan` (`unchanged`, `skipped`), `ReplacementPlan` (`installedVersion`), `ForgetResult`, `UninstallPlan` (`dependencyConflict`) and fork results.

#### Scenario: Warnings reach the host
- **WHEN** an install skips a broken historical tag and ignores a wrong-typed metadata key
- **THEN** the FFI `InstallResult` carries both in `skipped` and `warnings`, as the CLI prints them; an update past such a tag carries it in `RevisionChange.skipped`, `update all` in `UpdateAllPlan.skipped`

#### Scenario: Fork results
- **WHEN** a host reads `fork` or `forkPackages` results
- **THEN** the records mirror the library fork repository and package data without release/install/replace outcomes, failed steps or pending resume plans

#### Scenario: Partial fork failure
- **WHEN** an ordinary fork operation fails before its editable repository is complete
- **THEN** the host receives the library error with its kind and message rather than a release/install/replace step outcome
- **AND** no install step or pending resume plan is produced
