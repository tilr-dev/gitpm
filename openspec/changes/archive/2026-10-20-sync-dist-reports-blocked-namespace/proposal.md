# Proposal: sync-dist reports packages below a blocked namespace path

## Why

Active packages below a `dist/<ns>` path that is a symlink or a file are left out of `DistSync.skipped`, so `sync-dist` exits 0 and prints "dist/ already holds exactly the active packages." while a package is not deployed (review 2, G1#18, TILR-51).

## What Changes

- `sync-dist` and every dist report list an active package below a blocked namespace path in `skipped` with the reason; `sync-dist` then fails with `not_deployed`.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `dist`: "Unknown paths are left alone".

## Impact

`workspace.rs` (dist report). Tracker: TILR-51. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
