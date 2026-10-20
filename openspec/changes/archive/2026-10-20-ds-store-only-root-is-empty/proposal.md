# Proposal: A folder with only .DS_Store counts as empty

## Why

A fresh folder made in Finder holds `.DS_Store` and is refused as a non-empty foreign directory when a workspace is created in it; Tilr runs on macOS (review 2, G1#24, TILR-136).

## What Changes

- Opening a workspace root that holds only `.DS_Store` treats it as empty and creates the workspace.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Non-goals

- Do not remove Finder metadata, accept other foreign content, or change workspace initialization and recovery.

## Capabilities

### Modified Capabilities
- `workspace-layout`: "Foreign directories are refused".

## Impact

`workspace.rs` (`open`). Tracker: TILR-136. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
