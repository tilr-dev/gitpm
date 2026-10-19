## MODIFIED Requirements

### Requirement: Crash recovery
Commit SHALL fsync the journal before the first rename, and `state.json` and the parents of moved paths before it returns, so a returned commit survives a crash (process death or power loss). On open and at the start of a transaction, a committed journal SHALL be rolled forward and an uncommitted transaction discarded. Verified by `workspace::open_rolls_an_interrupted_commit_forward` and `workspace::open_discards_an_uncommitted_transaction`.

#### Scenario: Crash during commit
- **WHEN** the process dies after the journal is written but before all renames
- **THEN** the next open completes the commit and state and `dist/` are consistent

#### Scenario: Power loss after success
- **WHEN** an install returned successfully and the machine loses power right after
- **THEN** after reboot `state.json`, the clone and `dist/<id>` of the install are present
- **AND** this is covered by `txn::commit_syncs_journal_state_and_parents` (fsync calls observed); recovery of `update all` by `update_all_recovers_every_plugin_after_an_interrupted_commit`
