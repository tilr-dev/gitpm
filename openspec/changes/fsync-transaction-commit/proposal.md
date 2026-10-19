# Proposal: fsync at transaction commit

## Why

Workspace transactions never fsync, so recovery covers process death but not power loss (review 2, G1#5, TILR-54).

## What Changes

- Commit fsyncs the journal before the first rename, and `state.json` and the parents of moved paths before it returns; "crash" means process death or power loss.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `transactions-locking`: "Crash recovery".

## Impact

`txn.rs` (commit), README rules (crash definition). Tracker: TILR-54. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
