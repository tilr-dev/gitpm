## ADDED Requirements

### Requirement: Package inspection
Library, FFI and CLI SHALL expose working-tree packages of a repository, including a root package without a declared package set. Results SHALL carry ID, name, version, path, optional forkOf, dependency IDs and requirements, and installation membership by ID in an optional workspace. Inspection SHALL NOT mutate a repository or create or lock a workspace. Existing installed listing SHALL add name and installed true while retaining dependency status details and optional-field omissions.

#### Scenario: Repository packages before installation
- **WHEN** a caller inspects a repository with uninstalled original and fork packages
- **THEN** every declared package is returned with its manifest identity, name, version, path, provenance and dependency requirements, with installed false
- **AND** omitting the workspace root reports every package uninstalled
- **AND** inspection requires no built output

#### Scenario: Installed forks
- **WHEN** a caller lists a workspace containing an installed fork
- **THEN** its entry includes its name, forkOf, dependency IDs and requirements, and installed true
- **AND** repository inspection with that workspace marks the matching ID installed

### Requirement: Explicit dependency editing
Library, FFI and CLI SHALL add, update or remove a dependency of one package selected by ID in an author repository, or an installed editable Git worktree through the locked manager. Callers SHALL supply IDs and requirements explicitly. The complete resulting manifest SHALL be validated before writing. Edits SHALL preserve other fields and unknown keys, changing no other manifest, recorded state or deployed output. Successful results SHALL describe the edited package.

#### Scenario: Add, update and remove
- **WHEN** a caller adds a dependency, changes its requirement or ID, then removes it, in an author repository or installed fork
- **THEN** each successful edit appears in the selected working-tree manifest and returned description
- **AND** unrelated packages, metadata, state and deployed output remain unchanged
- **AND** no built output is required for author edits

#### Scenario: CLI editing forms
- **WHEN** a caller uses `dependency add PACKAGE DEPENDENCY REQUIREMENT`, `dependency update PACKAGE OLD NEW REQUIREMENT` or `dependency remove PACKAGE DEPENDENCY`
- **THEN** `--repo DIR` selects an author repository; without it the configured workspace selects an installed package
- **AND** update accepts the same old and new ID to change only the requirement

#### Scenario: Invalid edit leaves bytes intact
- **WHEN** a caller adds an existing dependency, updates or removes a missing dependency, selects a missing package, or updates to an occupied destination ID
- **THEN** the operation fails with invalid_argument and all manifest bytes remain unchanged
- **AND** a self-dependency fails with invalid_manifest, an invalid dependency UUID with invalid_id, and an invalid requirement with invalid_manifest, without writes

#### Scenario: Fork then inspect and choose a sibling dependency
- **WHEN** a caller forks core and cli, inspects cli's unchanged dependency on the original core, and explicitly updates its ID to the forked core with a chosen requirement
- **THEN** cli depends on the chosen fork ID and requirement while core and unrelated dependencies remain unchanged
- **AND** no release, installation or replacement is performed implicitly
