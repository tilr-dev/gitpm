## MODIFIED Requirements

### Requirement: Crash recovery
A crash means process death or power loss, including interruption before a call returns. Commit SHALL sync only gitpm's journal, state and their publication/cleanup directories, leaving repository data durability to Git and the OS. On open and at transaction start, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Sync failures SHALL return errors without losing the recovery decision.

#### Scenario: Durable commit ordering
- **WHEN** a transaction commits
- **THEN** staged `state.json` is synced before journal publication
- **AND** the journal, its containing transaction directory and the workspace root are synced before the first managed-path rename
- **AND** publishing `state.json` syncs its source transaction directory and destination workspace root; managed repository, worktree and dist renames do not trigger fsync
- **AND** transaction cleanup is persisted before returning success

#### Scenario: Workspace ancestors
- **WHEN** a workspace is created under missing parent directories
- **THEN** creation does not sync new workspace ancestors
- **AND** subsequent commits sync only metadata files and their containing directories within the workspace
- **AND** a pre-existing ancestor above the workspace parent need not be readable
- **AND** this is covered by `txn::workspace_creation_does_not_sync_ancestors` and `commit_under_an_unreadable_ancestor`

#### Scenario: Open after interruption
- **WHEN** a workspace is opened after an interrupted transaction
- **THEN** a committed journal is rolled forward and an uncommitted transaction discarded
- **AND** recovery is verified by `workspace::open_rolls_an_interrupted_commit_forward`, `workspace::open_discards_an_uncommitted_transaction` and `update_all_recovers_every_plugin_after_an_interrupted_commit`

#### Scenario: Crash during commit
- **WHEN** the process is interrupted after journal publication but before all renames
- **THEN** the next open completes the commit and state and `dist/` are consistent

#### Scenario: Power loss after success
- **WHEN** an install returned successfully and the machine loses power right after
- **THEN** gitpm has synced `state.json` and its publication directory entries before returning success
- **AND** repository, worktree and dist contents after reboot depend on Git and the operating system
- **AND** this is covered by `txn::commit_syncs_only_journal_state_and_metadata_parents` (fsync calls observed); recovery of `update all` by `update_all_recovers_every_plugin_after_an_interrupted_commit`

#### Scenario: Sync failure
- **WHEN** a journal, state or metadata-directory sync fails during commit
- **THEN** commit returns the error
- **AND** before journal publication the live workspace is unchanged
- **AND** after journal publication the next open rolls the transaction forward, even when a rename already succeeded
- **AND** this is covered by `txn::sync_errors_preserve_the_commit_decision_and_recovery`
