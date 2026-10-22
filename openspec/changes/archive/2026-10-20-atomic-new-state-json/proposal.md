# Proposal: Write a new state.json atomically

## Why

A new workspace's `state.json` is written with a plain write; a crash during it leaves a partial `state.json`, which must never be overwritten, so the workspace is stuck (review 2, G1#24, TILR-52).

## What Changes

- A new `state.json` is written to a temporary file and renamed into place.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `workspace-layout`: "Workspace folders" (scenario "Crash while creating state.json" moved here from "Foreign directories are refused").

## Impact

`workspace.rs` (`open`). Tracker: TILR-52. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).

## Supervisor follow-up

Reuse one atomic file writer for catalog and initial workspace state. The initial
state is staged inside the root, so creating a workspace on a mount-point root keeps
working. On open, a root that holds only abandoned initial-state temporaries is
cleaned under the workspace lock; any other entry still refuses the directory
without touching it. Deterministic tests cover the interrupted write and a
cross-device rename.
