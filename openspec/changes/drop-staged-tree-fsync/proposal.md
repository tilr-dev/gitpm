# Sync transaction metadata only

## Why

Syncing every staged repository and worktree file dominates transaction and test runtime. Git and the operating system manage repository data durability.

## What Changes

- Keep journal-based commit and recovery, syncing only journal/state files and their publication directories.
- Remove staged-tree, worktree-link, managed-path parent-chain and workspace-ancestor fsync.
- Document the reduced power-loss guarantee and verify metadata sync ordering.

## Capabilities

### Modified Capabilities
- `transactions-locking`: Crash recovery.

## Impact

Transaction implementation, durability tests and library rules in README.
