# transactions-locking Specification

## Purpose

Defines how workspace changes stay atomic and serialized: journaled transactions that stage only touched paths, crash recovery, the process-owned workspace lock, in-process serialization and managed-path safety.

## Requirements

### Requirement: Transactions stage only touched paths
Mutations SHALL stage bare repositories, package worktrees, output and next state inside `.pm-txn`, leaving live paths untouched before commit. Commit SHALL journal changes, move replaced live paths to `.pm-txn/old`, publish staged paths and repair Git links, then atomically rename state last (errors, "Failing operations leave the workspace unchanged"). Untouched repositories SHALL NOT be rewritten except for keep-alive refs as described below.

#### Scenario: Keep-alive refs after commit
- **WHEN** a transaction changes a package of a shared clone
- **THEN** after commit its keep-alive refs are written in the live clone (multi-package-repos, "Keep-alive refs")

#### Scenario: Deployed folders
- **WHEN** any mutating operation commits
- **THEN** the deployed folders it adds or removes are those of dist ("Every mutating command reconciles all of dist")

#### Scenario: Forget in a shared clone
- **WHEN** forgetting a Git package that shares a bare repository
- **THEN** its worktree is retained and other worktrees stay unchanged

#### Scenario: Readers during commit
- **WHEN** another process reads `state.json` while a commit runs
- **THEN** it always sees a complete file

#### Scenario: Failure before commit
- **WHEN** an install fails while staging
- **THEN** nothing staged has been moved into the live workspace and `.pm-txn` is gone

#### Scenario: Relocated worktrees
- **WHEN** a staged bare repository or worktree is committed or recovered after interruption
- **THEN** worktree Git links resolve to live managed paths, all retained worktrees remain usable, and no link depends on a removed transaction directory

### Requirement: Crash recovery
A crash means process death or power loss, including interruption before a call returns. Commit SHALL durably persist staged contents, journal, state and changed directory entries before success. On open and at transaction start, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Sync failures SHALL return errors without losing the recovery decision.

#### Scenario: Durable commit ordering
- **WHEN** a transaction commits
- **THEN** staged files and directories, including `state.json`, are synced before journal publication
- **AND** the journal and its parent directories are synced before the first managed-path rename
- **AND** each rename syncs destination and source parent chains before proceeding
- **AND** transaction cleanup is persisted before returning success

#### Scenario: Workspace ancestors
- **WHEN** a workspace is created under missing parent directories
- **THEN** creation syncs each new directory and its parent, stopping after the first pre-existing parent
- **AND** subsequent commits sync directory chains only within the workspace
- **AND** a pre-existing ancestor above the workspace parent need not be readable
- **AND** this is covered by `txn::workspace_creation_syncs_only_new_ancestors` and `commit_under_an_unreadable_ancestor`

#### Scenario: Open after interruption
- **WHEN** a workspace is opened after an interrupted transaction
- **THEN** a committed journal is rolled forward and an uncommitted transaction discarded
- **AND** recovery is verified by `workspace::open_rolls_an_interrupted_commit_forward`, `workspace::open_discards_an_uncommitted_transaction` and `update_all_recovers_every_plugin_after_an_interrupted_commit`

#### Scenario: Crash during commit
- **WHEN** a crash occurs after the journal is durably published but before all renames
- **THEN** the next open completes the commit and state and `dist/` are consistent

#### Scenario: Power loss after success
- **WHEN** an install returned successfully and the machine loses power right after
- **THEN** after reboot `state.json`, the clone and `dist/<id>` of the install are present
- **AND** this is covered by `txn::commit_syncs_journal_state_and_parents` (fsync calls observed); recovery of `update all` by `update_all_recovers_every_plugin_after_an_interrupted_commit`

#### Scenario: Sync failure
- **WHEN** a sync fails during commit
- **THEN** commit returns the error
- **AND** before journal publication the live workspace is unchanged
- **AND** after journal publication the next open rolls the transaction forward, even when a rename already succeeded
- **AND** this is covered by `txn::sync_errors_preserve_the_commit_decision_and_recovery`

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
Every directory between the root and a managed path (`repos`, `worktrees`, `dist`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed` and `a_uuid_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Symlinked namespace
- **WHEN** `repos` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `6d082077-2279-5cc6-b47f-b78cd35cec24` refuse and nothing outside the workspace root changes

#### Scenario: Blocked dist namespace
- **WHEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced` is a file or symlink and `d135a7cf-1c63-549a-93cb-8f89656b2ced` is active
- **THEN** an operation that deploys or removes `d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `workspace` and does not write or delete through `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced`, while a command on an unrelated package can proceed

#### Scenario: Symlinked worktree parent
- **WHEN** `worktrees` or the affected `worktrees/<uuid>` entry is a symlink or a non-directory
- **THEN** an operation using that managed checkout path refuses with `workspace` and changes nothing through it

### Requirement: Link transactions own entries and never their targets
Transactions SHALL stage and journal the deployed symlink entry, then publish state last, under the existing recovery protocol. Cleanup, removal and crash recovery MUST NOT follow a deployed link or recurse into its target, including dangling or externally retargeted links. The leaf symlink exception MUST NOT relax "Managed paths are never followed through symlinks" for managed parent directories. Atomicity SHALL cover workspace entries and state, not externally edited content.

#### Scenario: Failure before commit
- **WHEN** a link operation fails before the committed journal is written
- **THEN** staged symlink entries are discarded, and the previous workspace and external folder remain unchanged

#### Scenario: Interrupted link commit
- **WHEN** a process stops after the journal commits and before all symlink-entry and state renames finish
- **THEN** recovery completes the recorded entry changes and state idempotently without traversing or modifying the author's folder

#### Scenario: External edits during recovery
- **WHEN** the author edits or removes the target after journaling and before recovery
- **THEN** recovery operates on workspace symlink entries only and does not restore, delete or overwrite external content

#### Scenario: Managed parent is a symlink
- **WHEN** `dist` is a symlink outside the workspace
- **THEN** link deployment or removal refuses with `workspace` without writing or deleting through that parent

### Requirement: Worktree removal avoids unnecessary bare copies
Removing a package worktree SHALL NOT stage a bare repository that is being deleted or has no registration for that package. Retained bare repositories without that registration SHALL remain in place.

#### Scenario: Last repository user is removed
- **WHEN** uninstall, forget of a missing worktree, or build cleanup removes the last user of a bare repository
- **THEN** the bare repository is deleted without creating a staged copy for worktree removal

#### Scenario: Build update retains a shared bare repository
- **WHEN** a build updates to another build while a sibling retains the source repository
- **AND** the build has no worktree registration
- **THEN** worktree cleanup does not copy or replace the retained bare repository
