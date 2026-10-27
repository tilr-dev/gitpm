# Proposal: A refused lock attempt keeps the held lock

## Why

`try_lock` opens the lock file before it checks the in-process set of held locks, so a refused second `Manager::open` in the same process drops its file descriptor and thereby releases the first Manager's POSIX record lock; the catalog lock has the same path (TILR-45).

## What Changes

- Lock attempts check the in-process set of held locks (canonical path) before opening the lock file, so a refused second open never releases a held workspace or catalog lock.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `transactions-locking`: "Locks are process-owned and survive crashes".

## Impact

`lock.rs` (in-process check first), catalog lock. Tracker: TILR-45. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
