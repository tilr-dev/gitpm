# workspace-layout Specification

## Purpose

Defines the on-disk workspace a `Manager` owns: its folders, `state.json` and its format version, what opening a workspace creates or refuses, and how state records installed, replaced and forgotten packages.

## Requirements

### Requirement: Workspace folders
A workspace root SHALL contain `state.json` (installed packages, replacement bindings, forgotten records), `packages/<ns>/<name>/` (own clone of a package from a single-package repository), `dist/<ns>/<name>/` (deployed folder of an active package), `repos/<name>-<hash>/` (shared clone of a multi-package repository; `repos/` only while one exists) and `.pm-txn/` only while a transaction runs or after one was interrupted. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

#### Scenario: A command on a missing workspace root
- **WHEN** a command that opens a workspace (transactions-locking, "Workspace lock") names a workspace root that does not exist
- **THEN** the workspace root is created first

#### Scenario: Opening a new workspace
- **WHEN** a Manager opens a root that does not exist
- **THEN** the workspace root is created with `packages/`, `dist/` and a `state.json` of `{"format": 3, "packages": {}}`, and the workspace lock file (transactions-locking, "Workspace lock") is created beside it

### Requirement: Foreign directories are refused
Opening a non-empty directory that has no `state.json` SHALL fail with `workspace` ("refusing nonempty directory without state.json") and change nothing inside that directory (its parent folder and the workspace lock file beside it may already exist). Single source for a corrupt `state.json`: it SHALL block every operation with `invalid_data` and MUST NOT be overwritten. Verified by `workspace::open_refuses_foreign_directories_and_bad_state` and `robustness::corrupt_state_is_not_overwritten`.

#### Scenario: Pointing the workspace at a project folder
- **WHEN** the workspace root is an existing non-empty folder without `state.json`
- **THEN** opening fails with `workspace` and no file is created in it

#### Scenario: Corrupt state
- **WHEN** `state.json` is not valid JSON
- **THEN** every operation fails with `invalid_data` and the file keeps its bytes

### Requirement: State format versions
`state.json` SHALL carry `"format": 3` and keep installed packages under the key `packages`. A file with any other format, or without one, SHALL be refused with `unsupported` and MUST NOT be rewritten; there is no migration. The format SHALL be bumped with every change that an older gitpm would misread. Verified by `model::state_format_other_than_current_is_refused`.

#### Scenario: Workspace from gitpm 0.3
- **WHEN** a Manager opens a workspace whose `state.json` has no `format`
- **THEN** opening fails with `unsupported`, the message names the found and the supported format, and `state.json` keeps its bytes

#### Scenario: Workspace of gitpm 0.5
- **WHEN** a workspace written by gitpm 0.4 or 0.5 (`"format": 2`, packages under `plugins`) is opened
- **THEN** opening fails with `unsupported` and `state.json` keeps its bytes

#### Scenario: Newer format
- **WHEN** `state.json` has a format number this gitpm does not write
- **THEN** opening the state fails with `unsupported`

### Requirement: Installed package record
Single source for the record format: each `state.json` `packages.<id>` record SHALL hold `source`, `current` and `previous` revisions (`version`, `commit`), `enabled`, `metadata` (required; a record without it is `invalid_data`), and when present `forkOf`, `dependencies`, `replaces`, `catalog` (canonical catalog location), `clone` (shared-clone key) and `path` (package folder). At most one previous revision SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed or restored
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (the replacement bindings ORIGINAL → FORK, global for the workspace; rules in fork-replacement) and `forgotten` (records of forgotten packages of shared clones, same format as `packages.<id>`; their lifecycle is in multi-package-repos). A forgotten package with its own clone needs no record: its clone `packages/<id>` stays and marks it retained.

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `tilr/store` (installed from a multi-package repository) is forgotten
- **THEN** `packages.tilr/store` is removed and `forgotten.tilr/store` keeps its revision and clone key

### Requirement: Downgrade is not protected
A workspace SHALL be usable only by a gitpm that writes its state format, in either direction. README SHALL list the state format of each release and say that a workspace of another format is re-created (its packages installed again), never migrated.

#### Scenario: Documented downgrade boundary
- **WHEN** a user reads README "Rules of the library"
- **THEN** it lists the state format per gitpm release and says that a workspace of another format is refused with `unsupported`
