## MODIFIED Requirements

### Requirement: Workspace folders
A workspace SHALL contain `state.json` (packages, replacements, forgotten records), `dist/<uuid>` (active deployed output), `repos/<source-key>` (bare source stores), `worktrees/<uuid>` (detached Git package worktrees), and `.pm-txn` only during transactions or recovery. `repos/` and `worktrees/` SHALL exist only while needed. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

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
- **THEN** verified output is retained under builds/ID and there is no bare clone or worktree unless another Git revision needs the source

#### Scenario: Build cache transactions
- **WHEN** a build install or update changes its retained output
- **THEN** `builds/<id>` keeps the verified current and previous output without a clone, and cache changes use workspace transactions

#### Scenario: Existing legacy clone directory
- **WHEN** a workspace contains a legacy `packages/` directory
- **THEN** opening does not migrate, consult or automatically remove its clones, and new Git installs use only `repos/`

### Requirement: Installed package record
Each `packages.<id>` record SHALL hold `source`, `current`, at most one `previous`, `enabled`, and optional `mode`, `forkOf`, `dependencies`, `replaces`, `catalog`, `clone`, `path`, `metadata`. Revisions hold `version` and `commit`; `previous` retains mode. Links follow "Link record format". Git worktree paths SHALL derive from UUID; `path` selects the package folder inside that tree. Builds SHALL own no worktree. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

#### Scenario: Recorded modes
- **WHEN** packages were installed without a ref, with `--ref v1.0.0`, and with `--ref feature`
- **THEN** the first record has no `mode`, the second `"mode": {"pinned": "v1.0.0"}`, the third `"mode": {"branch": "feature"}`

#### Scenario: Previous revision keeps its mode
- **WHEN** a package in branch mode `main` is updated with `--release`
- **THEN** `previous` is `{"version": …, "commit": …, "mode": {"branch": "main"}}` (a `previous` in release mode has no `mode`), so `rollback` restores both

#### Scenario: Previous build provenance
- **WHEN** a build revision is updated to a Git revision
- **THEN** previous_build retains the release metadata and catalog location for offline rollback

#### Scenario: Build record format
- **WHEN** a current or previous revision is a build
- **THEN** `build` or `previous_build` retains its full release record and catalog location separately from catalog protection; previous retains its mode and source kind for offline rollback, and only current/previous build output is retained

#### Scenario: Managed field semantics
- **WHEN** a managed package record includes optional fields
- **THEN** mode is `{"pinned": REF}` or `{"branch": NAME}` (omitted for releases), catalog protects a verified catalog installation, and clone is the source-derived Git-clone key

#### Scenario: UUID identity
- **WHEN** a package is recorded in state
- **THEN** its record is keyed only by UUID without an alias hint; format remains 1 and legacy scoped records are refused without migration

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (the replacement bindings ORIGINAL → FORK, global for the workspace; rules in fork-replacement) and `forgotten` (records of every forgotten package, same format as `packages.<id>`, with a source-keyed bare repository and package worktree, or retained build output; lifecycle in install-uninstall and multi-package-repos).

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` (installed from a multi-package repository) is forgotten
- **THEN** `packages.76a9b591-60f3-5c7f-9e1c-31648fc73bfd` is removed and `forgotten.76a9b591-60f3-5c7f-9e1c-31648fc73bfd` keeps its revision and clone key

#### Scenario: Forgetting a package with its own clone
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` (root package in `repos/<key>`) is forgotten
- **THEN** `forgotten.d135a7cf-1c63-549a-93cb-8f89656b2ced` keeps its record, including `catalog`, `enabled` and mode, and its bare repository and worktree stay

