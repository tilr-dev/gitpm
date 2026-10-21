## MODIFIED Requirements

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
