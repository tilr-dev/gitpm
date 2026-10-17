# workspace-layout Specification

## Purpose

Defines the on-disk workspace a `Manager` owns: its folders, `state.json` and its format versions, what opening a workspace creates or refuses, and how state records installed, replaced and forgotten packages.

## Requirements

### Requirement: Workspace folders
A workspace root SHALL contain `state.json` (installed packages, replacements, forgotten records), `plugins/` with `plugins/<ns>/<name>/` (own clone of a root package), `dist/` with `dist/<ns>/<name>/` (deployed files of an active package), `repos/<name>-<hash>/` (shared clone of a multi-package source; `repos/` exists only while such a clone exists) and, only while an operation is interrupted, `.pm-txn/`. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

#### Scenario: Opening a new workspace
- **WHEN** a Manager opens a root that does not exist
- **THEN** the root is created with `plugins/`, `dist/` and a `state.json` of `{"format": 2, "plugins": {}}`, and the lock file `.<root name>.lock` is created beside it

### Requirement: Foreign directories are refused
Opening a non-empty directory that has no `state.json` SHALL fail with `workspace` ("refusing nonempty directory without state.json") and change nothing inside that directory (its parent folder and the lock file `.<name>.lock` beside it may already have been created). A corrupt `state.json` SHALL block every operation and MUST NOT be overwritten. Verified by `workspace::open_refuses_foreign_directories_and_bad_state` and `robustness::corrupt_state_is_not_overwritten`.

#### Scenario: Pointing the workspace at a project folder
- **WHEN** the root is an existing non-empty folder without `state.json`
- **THEN** opening fails with `workspace` and no file is created in it

#### Scenario: Corrupt state
- **WHEN** `state.json` is not valid JSON
- **THEN** every operation fails with `invalid_data` and the file keeps its bytes

### Requirement: State format versions
`state.json` SHALL carry `"format": 2` (`STATE_FORMAT`). A file without `format` SHALL be read as format 1 (gitpm 0.3 and earlier) and rewritten as format 2 on the next write; a newer format SHALL be refused with `unsupported`. Verified by `model::state_formats_one_and_two_are_read_newer_ones_refused` and `workspace::a_workspace_of_gitpm_0_3_opens_and_is_written_as_format_2`.

#### Scenario: Workspace from gitpm 0.3
- **WHEN** a 0.4+ Manager opens a workspace whose `state.json` has no `format`
- **THEN** it opens, and the next mutating command writes `"format": 2`

#### Scenario: Newer format
- **WHEN** `state.json` has `"format": 3`
- **THEN** opening the state fails with `unsupported`

### Requirement: Installed package record
This requirement is the single source for the record format. Each `state.json` `plugins.<id>` record SHALL hold `source`, `current` and `previous` revisions (`version`, `commit`), `enabled`, and when present `forkOf`, `dependencies`, `replaces`, `catalog` (canonical catalog location), `clone` (shared-clone key), `path` (package folder) and `metadata`. Exactly one previous revision SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (original ID → installed provider ID, global for the workspace) and `forgotten` (records of forgotten packages of shared clones, same format as `plugins.<id>`; their lifecycle is in multi-package-repos). A forgotten root package needs no record: its clone `plugins/<id>` stays and marks it retained.

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `tilr/store` (installed from a multi-package repository) is forgotten
- **THEN** `plugins.tilr/store` is removed and `forgotten.tilr/store` keeps its revision and clone key

### Requirement: Downgrade is not protected
gitpm SHALL NOT guarantee that an older gitpm can use a workspace written by a newer one; README SHALL document the boundary (0.4 workspaces with shared clones are unusable by 0.3).

#### Scenario: Documented downgrade boundary
- **WHEN** a user reads README "Rules of the library"
- **THEN** it states that 0.4 is not downgrade-safe and why (0.3 ignores `clone`/`path` and looks for `plugins/<id>`)
