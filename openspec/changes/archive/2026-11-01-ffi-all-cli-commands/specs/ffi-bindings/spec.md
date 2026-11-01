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

#### Scenario: Author operations and catalog writes
- **WHEN** a host needs `init`, `commit`, `remote`, `catalog update` or `catalog register`
- **THEN** free functions expose the same library operations with explicit repository paths, options and structured results, without requiring a workspace

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

### Requirement: CLI and FFI parity
Every CLI command and subcommand SHALL have an FFI equivalent that accepts the same operation parameters and returns the data the CLI prints, so a host never re-implements CLI logic. Author commands and catalog writers SHALL use the same library functions as the CLI. Verified by `ffi_covers_every_cli_command`.


#### Scenario: list with a filter
- **WHEN** a host needs what `gitpm list --provides theme` prints
- **THEN** it calls the free function `list(root, provides)` and gets the same state and `resolved` entries

#### Scenario: Restorable packages
- **WHEN** a host wants to offer `restore` for forgotten packages
- **THEN** `readState(root)` returns the full state view (raw state including `forgotten` and `replacements`, plus `resolved` with `active`) without opening the workspace; it is the only free function for the state

## ADDED Requirements

### Requirement: Native author operations
The free functions `init(directory, name, InitOptions)`, `commit(directory, version, package)` and `setRemote(directory, url)` SHALL call the same scaffold operations as the CLI, without opening a workspace. Options, result fields and library errors SHALL cross the boundary unchanged.

#### Scenario: Author input and result records
- **WHEN** a host uses author operations
- **THEN** `InitOptions` carries `multi` and `release` (defaults false and true); `InitResult` carries ID, canonical repository path, optional folder, version, new-repository flag and optional tag; `CommitResult` carries package, version and tag

#### Scenario: Create and release a multi-package repository
- **WHEN** a host creates a multi-package repository with `release` false, adds another package, then commits a selected package
- **THEN** init returns the generated identities and folders without an initial tag, and commit returns the selected package, version and repository-wide release tag
- **AND** no workspace state is created

#### Scenario: Configure the repository origin
- **WHEN** a host calls `setRemote` twice for a local author repository
- **THEN** the second call replaces the fetch and push origin without contacting the remote

#### Scenario: Invalid release version
- **WHEN** a host commits with an invalid version
- **THEN** the FFI preserves the library's invalid-version error

### Requirement: Native catalog writers
The free functions `catalogRegister(location, source, reference, package, build, customSsh)` and `catalogUpdate(location, sourcesFile, customSsh)` SHALL use the CLI's library operations with explicit locations. They SHALL preserve library validation, SSH policy, complete publication and update results, without opening a workspace.

#### Scenario: Catalog input and result records
- **WHEN** a host uses catalog writers
- **THEN** optional `CatalogBuild` contains the paired URL and SHA256; registration returns the typed release summary, lossless `releaseJson` with the complete release and warnings
- **AND** update returns the release count, source list, skipped tags with source/name/version/reason, and warnings

#### Scenario: Publish a package with a prebuilt archive
- **WHEN** a host registers a committed package with an explicit reference, package identity and HTTPS build metadata
- **THEN** the result retains the build metadata and complete release data, including custom manifest fields and file digests

#### Scenario: Update source releases
- **WHEN** a host updates a writable catalog from a sources file containing a local repository with valid and mismatched release tags
- **THEN** it receives the count and sources for valid releases, the skipped tag reason, and any publication warnings

#### Scenario: Writer refusals
- **WHEN** a host registers a build with an insecure URL or updates a verified catalog
- **THEN** it receives the same library refusal as the CLI

### Requirement: Command coverage checks native exports
The command coverage test SHALL compare every entry of the CLI `COMMANDS` table with the FFI mapping, without author or catalog-writer exceptions. It SHALL verify that mapped functions exist and carry UniFFI export annotations (directly or on their implementation block). Group entries such as `catalog` and `dependency` SHALL map to their native read or edit API.

#### Scenario: A command lacks a native export
- **WHEN** a new CLI command has no mapping, or a mapped function loses its UniFFI export annotation
- **THEN** the coverage test fails
