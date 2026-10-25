## MODIFIED Requirements

### Requirement: Workspace folders
A workspace SHALL contain `state.json` (packages, replacements, forgotten records), `dist/<ns>/<name>` (active deployed output), `repos/<source-key>` (bare source stores), `worktrees/<ns>/<name>` (detached Git package worktrees), and `.pm-txn` only during transactions or recovery. `repos/` and `worktrees/` SHALL exist only while needed. Verified by `workspace::open_creates_the_workspace_layout_and_lock`.

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
Each `state.json` `packages.<id>` managed record SHALL hold `source`, `current`, at most one `previous`, `enabled`, and optional `mode`, `forkOf`, `dependencies`, `replaces`, `catalog`, `clone`, `path`, `metadata`. Revisions contain `version` and `commit`; `previous` also retains mode. Links follow "Link record format". Git worktree paths SHALL derive from ID; `path` selects the package folder inside that tree. Builds SHALL own no worktree. Verified by `model::state_round_trips_byte_for_byte`.

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

#### Scenario: Managed field semantics
- **WHEN** a managed package record includes optional fields
- **THEN** mode is `{"pinned": REF}` or `{"branch": NAME}` (omitted for releases), catalog protects a verified catalog installation, and clone is the source-derived Git-clone key

### Requirement: Replacements and forgotten records
`state.json` SHALL keep `replacements` (the replacement bindings ORIGINAL → FORK, global for the workspace; rules in fork-replacement) and `forgotten` (records of every forgotten package, same format as `packages.<id>`, with a source-keyed bare repository and package worktree, or retained build output; lifecycle in install-uninstall and multi-package-repos).

#### Scenario: Forgetting a package of a shared clone
- **WHEN** `tilr/store` (installed from a multi-package repository) is forgotten
- **THEN** `packages.tilr/store` is removed and `forgotten.tilr/store` keeps its revision and clone key

#### Scenario: Forgetting a package with its own clone
- **WHEN** `acme/greeter` (root package in `repos/<key>`) is forgotten
- **THEN** `forgotten.acme/greeter` keeps its record, including `catalog`, `enabled` and mode, and its bare repository and worktree stay

### Requirement: Unified Git clone identity
Every Git package record SHALL include a source-derived `clone` key for a bare repository under `repos/`; `path` SHALL identify its folder within its package worktree and be absent for a root package. Clone-free build records SHALL remain valid without a clone key. A retained clone SHALL be reusable when switching a build back to Git without requiring the build commit to remain on the origin.

#### Scenario: Git clone identity
- **WHEN** a root or nested Git package is installed
- **THEN** its record includes the source-derived `clone` key; `path` is absent for the root and names the package folder otherwise
- **AND** a Git record without a clone key is invalid data; build-only records remain valid without a clone

#### Scenario: Retained clone after a build commit is removed upstream
- **WHEN** a Git install switches to a build, retains its clone, and the build commit is later removed from the origin
- **THEN** an explicit update to an available Git ref reuses the retained clone and succeeds without resolving the removed build commit first
- **AND** build provenance is retained for offline rollback

#### Scenario: Git to build and back
- **WHEN** a Git package switches to a build and later returns to Git, including by offline rollback
- **THEN** the build has no worktree, necessary Git objects remain reachable in the bare repository, and returning to Git creates a worktree at the target revision

## ADDED Requirements

### Requirement: Package worktrees are created on demand
Only a current Git installation SHALL own a package worktree, retained while forgotten. A build-only install SHALL create neither a worktree nor an unnecessary bare repository. Switching a Git installation to a build SHALL remove its worktree while retaining objects required for offline Git rollback. Legacy checked-out source clones SHALL NOT be migrated or automatically removed when opening a workspace.

#### Scenario: Fresh prebuilt install
- **WHEN** a package is installed from a verified catalog build with no retained Git revision
- **THEN** no package worktree or source bare repository is created

#### Scenario: Forgotten Git package
- **WHEN** a Git package is forgotten and restored
- **THEN** its own worktree is retained and reused at the recorded commit

#### Scenario: Build transition preserves unpublished source refs
- **WHEN** a retained build or a clone-free build beside a sibling switches to Git and fetching would discard unpublished refs in the reused bare repository
- **THEN** the transition refuses with `local_changes` and preserves refs and state, without guarding the sibling worktree's HEAD or files
