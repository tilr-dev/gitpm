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
- **THEN** the workspace root is created with `packages/`, `dist/` and a `state.json` of `{"format": 1, "packages": {}}` written to a temporary file and renamed into place, and the workspace lock file (transactions-locking, "Workspace lock") is created beside it

#### Scenario: Crash while creating state.json
- **WHEN** the process dies while a new workspace's `state.json` is written
- **THEN** the root holds no partial `state.json`, and the next open creates it again
- **AND** incomplete state is staged inside the root as `.state.json-<pid>-<counter>.tmp`, so the rename stays on one filesystem even when the root is a mount point
- **AND** a root that holds only such files (and possibly `.DS_Store`) is not a foreign directory: the next open removes them while holding the workspace lock and creates `state.json`; any other entry refuses the directory as "Foreign directories are refused" says, and nothing inside it is removed
- **AND** `.DS_Store` beside them keeps its bytes

#### Scenario: Clone-free build storage
- **WHEN** a catalog build is installed for the first time
- **THEN** verified output is retained under builds/ID and there is no package clone

#### Scenario: Build cache transactions
- **WHEN** a build install or update changes its retained output
- **THEN** `builds/<id>` keeps the verified current and previous output without a clone, and cache changes use workspace transactions

### Requirement: Foreign directories are refused
Opening a non-empty directory (one holding anything but `.DS_Store` and gitpm's own staged state `.state.json-<pid>-<counter>.tmp`, see "Workspace folders") that has no `state.json` SHALL fail with `workspace` ("refusing nonempty directory without state.json") and change nothing inside that directory (its parent folder and the workspace lock file beside it may already exist). Single source for a corrupt `state.json`: it SHALL block every operation with `invalid_data` and MUST NOT be overwritten.

#### Scenario: Pointing the workspace at a project folder
Verified by `workspace::open_refuses_foreign_directories_and_bad_state`.
- **WHEN** the workspace root is an existing non-empty folder without `state.json`
- **THEN** opening fails with `workspace` and no file is created in it (its parent and the lock file beside it may already exist)

#### Scenario: Corrupt state
Verified by `robustness::corrupt_state_is_not_overwritten`.
- **WHEN** `state.json` is not valid JSON
- **THEN** every operation fails with `invalid_data` and the file keeps its bytes

#### Scenario: Folder with only .DS_Store
Verified by `workspace::open_ds_store_only_root`.
- **WHEN** the workspace root exists and holds only `.DS_Store`
- **THEN** opening treats it as empty and creates the workspace in it
- **AND** `.DS_Store` keeps its bytes

#### Scenario: Folder with .DS_Store and other content
Verified by `workspace::open_ds_store_with_other_content_is_refused`.
- **WHEN** the workspace root holds `.DS_Store` and another file without `state.json`
- **THEN** opening fails with `workspace` and changes nothing inside it

### Requirement: State format versions
`state.json` SHALL carry `"format": 1` and keep installed packages under `packages`. Other or missing formats SHALL fail with `unsupported` without rewriting or migration. Before the first public release, changes SHALL stay in format 1 without backward compatibility. Later, only changes gitpm from the previous public release would misread SHALL cause a bump. Verified by `model::state_format_other_than_current_is_refused`.

#### Scenario: Release format bump
- **WHEN** a change would be misread by gitpm from the previous public release
- **THEN** the format is bumped as a separate step in the release milestone, at most once per release
- **AND** before the first public release, changes are made in place under format 1 without bumps or backward compatibility

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
Each `state.json` `packages.<id>` record is the single source for its format and SHALL hold `source`, `current` and `previous` revisions (`version`, `commit`; `previous` also `mode`), `enabled`, `mode` (`{"pinned": REF}`, `{"branch": NAME}`, omitted for releases), and optional `forkOf`, `dependencies`, `replaces`, protecting `catalog`, shared `clone`, `path` and `metadata`. At most one previous SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

#### Scenario: Recorded modes
- **WHEN** packages were installed without a ref, with `--ref acme/a@1.0.0`, and with `--ref feature`
- **THEN** the first record has no `mode`, the second `"mode": {"pinned": "acme/a@1.0.0"}`, the third `"mode": {"branch": "feature"}`

#### Scenario: Previous revision keeps its mode
- **WHEN** a package in branch mode `main` is updated with `--release`
- **THEN** `previous` is `{"version": …, "commit": …, "mode": {"branch": "main"}}` (a `previous` in release mode has no `mode`), so `rollback` restores both

#### Scenario: Previous build provenance
- **WHEN** a build revision is updated to a Git revision
- **THEN** previous_build retains the release metadata and catalog location for offline rollback

#### Scenario: Build record format
- **WHEN** a current or previous revision is a build
- **THEN** `build` or `previous_build` retains its full release record and catalog location separately from catalog protection; previous retains its mode and source kind for offline rollback, and only current/previous build output is retained

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (the replacement bindings ORIGINAL → FORK, global for the workspace; rules in fork-replacement) and `forgotten` (records of every forgotten package, same format as `packages.<id>`, own clone or shared clone alike; lifecycle in install-uninstall and multi-package-repos).

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `tilr/store` (installed from a multi-package repository) is forgotten
- **THEN** `packages.tilr/store` is removed and `forgotten.tilr/store` keeps its revision and clone key

#### Scenario: Forgetting a package with its own clone
- **WHEN** `acme/greeter` (own clone `packages/acme/greeter`) is forgotten
- **THEN** `forgotten.acme/greeter` keeps its record, including `catalog`, `enabled` and mode, and the clone stays

### Requirement: Downgrade is not protected
A workspace SHALL be usable only by a gitpm that writes its state format, in either direction. README SHALL list the state format of each release and say that a workspace of another format is re-created (its packages installed again), never migrated.

#### Scenario: Documented downgrade boundary
- **WHEN** a user reads README "Rules of the library"
- **THEN** it lists the state format per gitpm release and says that a workspace of another format is refused with `unsupported`

### Requirement: Options are validated before anything is created
`Manager::open_with` SHALL validate its root path and Options (for example the catalog location, catalog "A configured catalog location must exist") before it creates the workspace root, its folders, `state.json` or the lock file; a refusal SHALL leave the file system unchanged. Verified by `workspace::open_with_validates_options_before_creating_anything`.

#### Scenario: Invalid options on a new root
- **WHEN** a host opens `new-ws`, which does not exist, with an invalid Options value
- **THEN** opening fails, and neither `new-ws` nor `.new-ws.lock` exists afterwards

### Requirement: Workspaces are isolated
Every public workspace operation SHALL take an explicit root, directly or through a Manager or FFI handle. CLI workspace commands SHALL accept `--root`; the documented default MAY remain. State, clones, deployed files, builds, transactions and locks SHALL belong to the selected root. Operations SHALL NOT change another workspace's state or files. Repository-only operations SHALL use explicit repository paths without opening an implicit workspace.

#### Scenario: Preparing a second workspace
- **GIVEN** two workspaces with the same root basename in different parent directories containing the same package IDs
- **WHEN** a package is installed or removed in one workspace
- **THEN** the other workspace's state and files keep their contents
- **AND** both workspaces can remain open through independent Manager handles

#### Scenario: Explicit command selection
- **GIVEN** the process working directory and the environment point at a different workspace
- **WHEN** an existing public CLI workspace command is invoked with an explicit `--root`
- **THEN** the command uses the selected root
- **AND** the workspace selected by the working directory or environment keeps its state and files

#### Scenario: Repository-only operation
- **WHEN** a repository-only command receives an explicit repository path
- **THEN** it operates on that path without opening a default workspace
