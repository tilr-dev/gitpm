# transactions-locking Specification

## Purpose

Defines how workspace changes stay atomic and serialized: journaled transactions that stage only touched paths, crash recovery, the process-owned workspace lock, in-process serialization and managed-path safety.

## Requirements

### Requirement: Transactions stage only touched paths
Every mutating operation SHALL prepare new clones, deployed folders and the next `state.json` inside `<workspace root>/.pm-txn`, leaving the live workspace untouched until commit (mechanism of errors, "Failing operations leave the workspace unchanged"). Commit SHALL write a journal, then move each staged path into place (replaced ones go to `.pm-txn/old`), with `state.json` last via an atomic rename. A clone that an operation does not replace SHALL NOT be rewritten, except as in the scenarios.

#### Scenario: Keep-alive refs after commit
- **WHEN** a transaction changes a package of a shared clone
- **THEN** after commit its keep-alive refs are written in the live clone (multi-package-repos, "Keep-alive refs")

#### Scenario: Deployed folders
- **WHEN** any mutating operation commits
- **THEN** the deployed folders it adds or removes are those of dist ("Every mutating command reconciles all of dist")

#### Scenario: Forget in a shared clone
- **WHEN** forgetting a package whose shared clone's HEAD is at its revision
- **THEN** the clone's HEAD also moves to a remaining package's revision (see multi-package-repos, "Shared clone HEAD handling")

#### Scenario: Readers during commit
- **WHEN** another process reads `state.json` while a commit runs
- **THEN** it always sees a complete file

#### Scenario: Failure before commit
- **WHEN** an install fails while staging
- **THEN** nothing staged has been moved into the live workspace and `.pm-txn` is gone

### Requirement: Crash recovery
On open and at the start of a transaction, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Verified by `workspace::open_rolls_an_interrupted_commit_forward`, `workspace::open_discards_an_uncommitted_transaction` and `update_all_recovers_every_plugin_after_an_interrupted_commit`.

#### Scenario: Crash during commit
- **WHEN** the process dies after the journal is written but before all renames
- **THEN** the next open completes the commit and state and `dist/` are consistent

### Requirement: Neighbouring folders are never touched
Recovery SHALL only touch `.pm-txn` inside the workspace root, never folders beside the workspace named like old stage folders. Verified by `workspace::neighbours_named_like_old_stage_folders_are_never_touched`.

#### Scenario: Look-alike folder
- **WHEN** a folder `.ws.stage` exists beside workspace `ws`
- **THEN** opening `ws` leaves it alone

### Requirement: Workspace lock
Single source for which commands open a workspace: a Manager SHALL hold an exclusive lock on `.<workspace root name>.lock` beside the workspace root for its lifetime, keyed by the real (canonical) path so a symlinked spelling takes the same lock. A second Manager on the same workspace SHALL fail with `locked` ("workspace is already in use"). Read-only views SHALL NOT open a Manager. Verified by `recovery_and_lock` and `workspace::symlinked_workspace_root_shares_the_lock`.

#### Scenario: Who opens a workspace
- **WHEN** a CLI workspace command runs or an FFI `PackageManager` method is called
- **THEN** it opens a Manager: it takes the lock, creates a missing workspace root (workspace-layout) and runs crash recovery; the CLI `list` and `check` and the FFI free functions `list`, `check` and `readState` read `state.json` without opening, locking or creating anything

#### Scenario: Two CLI processes
- **WHEN** one `gitpm` process is installing and another runs `gitpm enable` on the same workspace root
- **THEN** the second fails with `locked`, while `gitpm list` in the meantime succeeds and reports the last committed `state.json`

### Requirement: Locks are process-owned and survive crashes
Workspace and catalog locks SHALL be process-owned POSIX record locks, not inherited by children and released when the owner dies. A process-wide set keyed by canonical path SHALL refuse repeated attempts before opening the file, preserving the held lock. Lock files SHALL be kept.

#### Scenario: Killed owner
- **WHEN** the process holding the lock is killed
- **THEN** the next open succeeds immediately

#### Scenario: Refused second Manager in the same process
- **WHEN** a host holds a Manager on a workspace, opens a second Manager on the same workspace in the same process (also through a symlinked spelling or a lock-file symlink whose target was initially missing), gets `locked`, and another process then opens the workspace
- **THEN** the other process still fails with `locked`, because the refused attempt did not release the first Manager's lock

#### Scenario: Refused second catalog writer in the same process
- **WHEN** a catalog write holds `.<catalog name>.catalog.lock` and the same process starts a second write to that catalog, which fails with `locked`
- **THEN** a writer in another process still fails with `locked` until the first write ends

#### Scenario: Lock regression coverage
- **WHEN** lock behavior is tested
- **THEN** `lock::lock_is_exclusive_across_processes_and_released_by_a_dead_owner`, `lock::released_lock_is_free_while_other_threads_spawn_subprocesses`, `lock::refused_second_open_keeps_the_first_lock`, and `catalog::tests::refused_second_catalog_writer_keeps_the_first_lock` verify process ownership, release, and refused attempts

### Requirement: One Manager serializes its operations
`Manager` SHALL be `Send + Sync` and SHALL serialize its operations internally; the API is blocking and callers run it off the UI thread. Verified by `workspace::manager_is_send_and_sync` and `workspace::one_handle_serializes_concurrent_operations`.

#### Scenario: Concurrent calls on one handle
- **WHEN** two threads call `install` and `disable` on the same Manager
- **THEN** they run one after the other and state is consistent

### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`packages`, `packages/<ns>`, `repos`, `dist`, `dist/<ns>`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed`.

#### Scenario: Symlinked namespace
- **WHEN** `packages/dan` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `dan/x` refuse and nothing outside the workspace root changes
