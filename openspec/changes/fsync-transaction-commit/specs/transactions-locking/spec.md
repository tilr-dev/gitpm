## MODIFIED Requirements

### Requirement: Crash recovery
A crash means process death or power loss, including interruption before a call returns. Commit SHALL sync staged file contents and directories, including `state.json`, before publishing its journal. It SHALL fsync the journal and its parent directories before the first managed-path rename, and persist destination and source parent directories after each rename before proceeding. Commit SHALL persist transaction cleanup before returning, so a returned commit survives a subsequent crash. Sync errors SHALL be returned; a published journal SHALL remain recoverable until all managed paths and state are durable. On open and at the start of a transaction, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Verified by `workspace::open_rolls_an_interrupted_commit_forward` and `workspace::open_discards_an_uncommitted_transaction`.

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
