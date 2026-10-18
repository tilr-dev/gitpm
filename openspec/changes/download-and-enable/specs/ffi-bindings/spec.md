## MODIFIED Requirements

### Requirement: Manager surface
`PackageManager` SHALL be opened with `open(root, ManagerOptions)` and expose every workspace operation that takes the lock (catalog reads, install, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, switch back, sync-dist, with their plans); free functions SHALL be `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`. Author and catalog-writing commands are not exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PackageManager` method, or one of the free functions `list`, `check`, `readState`
- **THEN** the method works on the opened (locked) workspace, while the free functions read `state.json` without opening, locking or creating it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview or the links of one catalog level
- **THEN** `PackageManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions `browseCatalog`, `catalogRelease` and `catalogLinks` take an explicit location without opening a workspace

#### Scenario: Install and update targets
- **WHEN** a host calls `install(source, target, package)` or `update(id, target)`
- **THEN** `install` takes the `InstallTarget` enum `Release`, `Ref(reference)` or `Dev`, and `update` the `Target` enum with the same cases plus `Recorded` (following the recorded mode); the former `Channel` enum is gone

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PackageManager` has `install` (optional `package`), `installFromCatalog`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)`, `restore`, `uninstallPlan`, `uninstall(confirm, force)`, `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `forkReplaceResume`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

