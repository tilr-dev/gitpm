## MODIFIED Requirements

### Requirement: Workspace folders
A workspace root SHALL contain `state.json` (installed packages, replacement bindings, forgotten records), `dist/<ns>/<name>/` (deployed folder of an active package), `repos/<name>-<hash>/` (source-keyed clone for root and nested Git packages; `repos/` only while a clone exists) and `.pm-txn/` only while a transaction runs or after one was interrupted. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

#### Scenario: A command on a missing workspace root
- **WHEN** a command that opens a workspace (transactions-locking, "Workspace lock") names a workspace root that does not exist
- **THEN** the workspace root is created first

#### Scenario: Opening a new workspace
- **WHEN** a Manager opens a root that does not exist
- **THEN** the workspace root is created with `dist/` and a `state.json` of `{"format": 1, "packages": {}}` written to a temporary file and renamed into place, and the workspace lock file (transactions-locking, "Workspace lock") is created beside it

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

#### Scenario: Existing legacy clone directory
- **WHEN** a workspace contains a legacy `packages/` directory
- **THEN** opening does not migrate, consult or automatically remove its clones, and new Git installs use only `repos/`

### Requirement: Installed package record
Each `state.json` `packages.<id>` record is the single source for its format and SHALL hold `source`, `current` and `previous` revisions (`version`, `commit`; `previous` also `mode`), `enabled`, `mode` (`{"pinned": REF}`, `{"branch": NAME}`, omitted for releases), and optional `forkOf`, `dependencies`, `replaces`, protecting `catalog`, `clone`, `path` and `metadata`. At most one previous SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

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

#### Scenario: Retained clone after a build commit is removed upstream
- **WHEN** a Git install switches to a build, retains its clone, and the build commit is later removed from the origin
- **THEN** an explicit update to an available Git ref reuses the retained clone and succeeds without resolving the removed build commit first
- **AND** build provenance is retained for offline rollback

#### Scenario: Build record format
- **WHEN** a current or previous revision is a build
- **THEN** `build` or `previous_build` retains its full release record and catalog location separately from catalog protection; previous retains its mode and source kind for offline rollback, and only current/previous build output is retained

#### Scenario: Git clone identity
- **WHEN** a root or nested Git package is installed
- **THEN** its record includes the source-derived `clone` key; `path` is absent for the root and names the package folder otherwise
- **AND** a Git record without a clone key is invalid data; build-only records remain valid without a clone

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (the replacement bindings ORIGINAL → FORK, global for the workspace; rules in fork-replacement) and `forgotten` (records of every forgotten package, same format as `packages.<id>`, with a source-keyed Git clone or retained build output; lifecycle in install-uninstall and multi-package-repos).

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `tilr/store` (installed from a multi-package repository) is forgotten
- **THEN** `packages.tilr/store` is removed and `forgotten.tilr/store` keeps its revision and clone key

#### Scenario: Forgetting a root package
- **WHEN** `acme/greeter` (root package in `repos/<key>`) is forgotten
- **THEN** `forgotten.acme/greeter` keeps its record, including `catalog`, `enabled` and mode, and the clone stays
