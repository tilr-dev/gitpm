## MODIFIED Requirements

### Requirement: Manager surface
`PackageManager` SHALL open with `open(root, ManagerOptions)` and expose every locked workspace operation and plan, including dependency editing. Free functions SHALL include `repositoryPackages`, `editDependency`, `list`, `check`, `readState`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork` and `uninstallRefusal` (throws `UninstallPlan::refusal`).

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

#### Scenario: Explicit fork dependency rewrite
- **WHEN** a host calls `repositoryPackages(directory, root)` or `editDependency(directory, package, edit)`
- **THEN** the free functions return package descriptions with identity, name, provenance, requirements and installation status, or the edited manifest, without opening a workspace
- **AND** the manager exposes dependency edits for installed editable packages with the workspace lock
