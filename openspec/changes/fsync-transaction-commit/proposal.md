# Proposal: fsync at transaction commit

## Why

Workspace transactions never fsync, so recovery covers process death but not power loss (review 2, G1#5).

## What Changes

- Commit fsyncs the journal before the first rename, and `state.json` and the parents of moved paths before it returns; "crash" means process death or power loss.

## Capabilities

### Modified Capabilities
- `transactions-locking`: "Crash recovery".

## Impact

`txn.rs` (commit), README rules (crash definition). Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).

## Implementation

Staged payloads are persisted before the journal decision. Staged payloads and all parent entries are synced as required for power-loss durability; sync failures preserve the published recovery decision.

Workspace creation syncs newly created ancestor entries up to the first existing parent. Later commits sync only within the workspace, so unchanged ancestors do not require read permission.
