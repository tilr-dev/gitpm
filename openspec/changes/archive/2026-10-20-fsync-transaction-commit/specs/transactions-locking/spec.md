## MODIFIED Requirements

### Requirement: Crash recovery
A crash means process death or power loss, including interruption before a call returns. Commit SHALL durably persist staged contents, journal, state and changed directory entries before success. On open and at transaction start, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Sync failures SHALL return errors without losing the recovery decision.

#### Scenario: Durable commit ordering
- **WHEN** a transaction commits
- **THEN** staged files and directories, including `state.json`, are synced before journal publication
- **AND** the journal and its parent directories are synced before the first managed-path rename
- **AND** each rename syncs destination and source parent chains before proceeding
- **AND** transaction cleanup is persisted before returning success

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
