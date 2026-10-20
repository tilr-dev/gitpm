## MODIFIED Requirements

### Requirement: Workspace folders
A workspace root SHALL contain `state.json` (installed packages, replacement bindings, forgotten records), `packages/<ns>/<name>/` (own clone of a package from a single-package repository), `dist/<ns>/<name>/` (deployed folder of an active package), `repos/<name>-<hash>/` (shared clone of a multi-package repository; `repos/` only while one exists) and `.pm-txn/` only while a transaction runs or after one was interrupted. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

#### Scenario: A command on a missing workspace root
- **WHEN** a command that opens a workspace (transactions-locking, "Workspace lock") names a workspace root that does not exist
- **THEN** the workspace root is created first

#### Scenario: Opening a new workspace
- **WHEN** a Manager opens a root that does not exist
- **THEN** the workspace root is created with `packages/`, `dist/` and a `state.json` of `{"format": 3, "packages": {}}` written to a temporary file and renamed into place, and the workspace lock file (transactions-locking, "Workspace lock") is created beside it

#### Scenario: Crash while creating state.json
- **WHEN** the process dies while a new workspace's `state.json` is written
- **THEN** the root holds no partial `state.json`, and the next open creates it again
- **AND** incomplete state is staged inside the root as `.state.json-<pid>-<counter>.tmp`, so the rename stays on one filesystem even when the root is a mount point
- **AND** a root that holds only such files (and possibly `.DS_Store`) is not a foreign directory: the next open removes them while holding the workspace lock and creates `state.json`; any other entry refuses the directory as "Foreign directories are refused" says, and nothing inside it is removed
- **AND** `.DS_Store` beside them keeps its bytes

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
