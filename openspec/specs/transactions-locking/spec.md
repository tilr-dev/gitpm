# transactions-locking Specification

## Purpose

Defines how workspace changes stay atomic and serialized: journaled transactions that stage only touched paths, crash recovery, the process-owned workspace lock, in-process serialization and managed-path safety.

## Requirements

### Requirement: Transactions stage only touched paths
Every mutating operation SHALL prepare new clones, `dist/` folders and the next `state.json` inside `<root>/.pm-txn`, leaving the live workspace untouched until commit (the mechanism behind errors, "Failing operations leave the workspace unchanged"). Commit SHALL write a journal, then move each staged path into place (replaced ones go to `.pm-txn/old`), with `state.json` last via an atomic rename. Enable, disable and forget SHALL rewrite only `state.json` and the affected `dist/`.

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
Recovery SHALL only touch `.pm-txn` inside the root, never folders beside the workspace named like old stage folders. Verified by `workspace::neighbours_named_like_old_stage_folders_are_never_touched`.

#### Scenario: Look-alike folder
- **WHEN** a folder `.ws.stage` exists beside workspace `ws`
- **THEN** opening `ws` leaves it alone

### Requirement: Workspace lock
A Manager SHALL hold an exclusive lock on `.<root name>.lock` beside the workspace for its lifetime, keyed by the real (canonical) path so a symlinked spelling takes the same lock. A second Manager on the same workspace SHALL fail with `locked` ("workspace is already in use"). Verified by `recovery_and_lock` and `workspace::symlinked_workspace_root_shares_the_lock`.

#### Scenario: Two CLI processes
- **WHEN** one `gitpm` process is installing and another runs `gitpm list` on the same root
- **THEN** the second fails with `locked`

### Requirement: Locks are process-owned and survive crashes
The lock SHALL be a POSIX record lock owned by the process, not inherited by child processes (git subprocesses never keep it alive), released when the owner dies, and tracked in a process-wide set so a second lock attempt in the same process is refused. The lock file itself SHALL be kept. Verified by `lock::lock_is_exclusive_across_processes_and_released_by_a_dead_owner` and `lock::released_lock_is_free_while_other_threads_spawn_subprocesses`.

#### Scenario: Killed owner
- **WHEN** the process holding the lock is killed
- **THEN** the next open succeeds immediately

### Requirement: One Manager serializes its operations
`Manager` SHALL be `Send + Sync` and SHALL serialize its operations internally; the API is blocking and callers run it off the UI thread. Verified by `workspace::manager_is_send_and_sync` and `workspace::one_handle_serializes_concurrent_operations`.

#### Scenario: Concurrent calls on one handle
- **WHEN** two threads call `install` and `disable` on the same Manager
- **THEN** they run one after the other and state is consistent

### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`plugins`, `plugins/<ns>`, `repos`, `dist`, `dist/<ns>`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed`.

#### Scenario: Symlinked namespace
- **WHEN** `plugins/dan` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `dan/x` refuse and nothing outside the root changes
