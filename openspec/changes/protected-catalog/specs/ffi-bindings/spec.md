## MODIFIED Requirements

### Requirement: Manager surface
`PluginManager` SHALL be opened with `open(root, ManagerOptions)` and expose every workspace operation that takes the lock (catalog reads, install, updates, rollback, enable/disable, forget/restore, uninstall, fork, replace, switch back, sync-dist, with their plans); free functions SHALL be `list`, `check`, `readState`, `readInstalled`, `catalogLinks`, `authCheck`, `browseCatalog`, `catalogRelease`, `describeLostWork`. Author and catalog-writing commands are not exposed.

#### Scenario: Which calls open the workspace
- **WHEN** a host calls a `PluginManager` method, or one of the free functions `list`, `check`, `readState`, `readInstalled`
- **THEN** the method works on the opened (locked) workspace, while the free functions read `state.json` without opening, locking or creating it (transactions-locking, "Workspace lock"); `authCheck(source, customSsh)` takes the SSH setting explicitly

#### Scenario: Catalog reads with and without a workspace
- **WHEN** a host needs the catalog overview or the links of one catalog level
- **THEN** `PluginManager.browseCatalog`/`catalogRelease` read the catalog of `ManagerOptions.catalog`, and the free functions `browseCatalog`, `catalogRelease` and `catalogLinks` take an explicit location without opening a workspace

#### Scenario: Update channel
- **WHEN** a host calls `install(source, package, target)` or `update(id, target, detach)`
- **THEN** `target` is the `Target` enum `Release`, `Ref(reference)` or `Dev` (for `update` also `Recorded`, following the recorded mode); the former `Channel` enum is gone

#### Scenario: Round trip
- **WHEN** a host opens an empty workspace through the FFI and lists it
- **THEN** the list is empty and no error is raised
- **AND** this is covered by `manager_round_trip_on_empty_workspace`

#### Scenario: Method names
- **WHEN** Swift bindings are generated
- **THEN** `PluginManager` has `install` (optional `package`), `installFromCatalog`, `download`, `downloadFromCatalog`, `update`, `updateAllPlan`, `updateAll`, `rollback`, `enable(force)`, `disable(force)`, `forget(force)`, `restore`, `uninstallPlan`, `uninstall(confirmed, force)` (`confirmed` the lost-work items the user saw), `fork`, `forkAndReplace`, `forkPackages`, `forkPackagesAndReplace`, `forkReplaceResume`, `replacePlan`, `replace`, `switchBackPlan`, `switchBack`, `syncDistPlan` and `syncDist`; `check` is a free function, not a method

#### Scenario: Catalog generation is not exposed
- **WHEN** a host looks for `catalog-update`, `register`, `init` or `commit` in the bindings
- **THEN** none exist; those stay library/CLI functions

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field and SHALL drop no field, so a host sees everything the CLI prints (scenario "Record fields" lists the records). Verified by `records_convert_field_by_field`.

#### Scenario: Partial fork failure
- **WHEN** `forkAndReplace` fails at the install step
- **THEN** the result has `failedStep` `install` with the error kind and message, not a thrown error
- **AND** this is covered by `records_convert_field_by_field` and `fork_replace_result_reports_the_failed_step_with_kind_and_message`

#### Scenario: Warnings reach the host
- **WHEN** an install skips a broken historical tag and ignores a wrong-typed metadata key
- **THEN** the FFI `InstallResult` carries both in `skipped` and `warnings`, as the CLI prints them

#### Scenario: Record fields
- **WHEN** Swift bindings are generated
- **THEN** they include `InstalledPlugin` (`active`, `clone`, `path`, `mode`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `unchangedReason`), the dist report of every mutating result, `CatalogRelease`, `PluginSummary`, `DistSync`, `UpdateAllPlan` (`unchanged` with reason, `keptBack` with `reason`), `ReplacementPlan` (`installedVersion`, `forkEnabled`), `CheckReport` (`catalogVerified`, rows with `source`, upstream, replacements, unsatisfied dependencies) and fork results (`ForkStep`, `errorKind`, `errorMessage`, pending plan)

#### Scenario: check without a catalog from Swift
- **WHEN** a host calls `check(root, nil)` on a workspace with packages installed from Git
- **THEN** it receives a `CheckReport` whose rows have `source` `origin`, plus the replacement bindings and unsatisfied dependencies the CLI prints

