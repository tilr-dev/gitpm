# Proposal: Refuse a submodule inside a package dist

## Why

A Git submodule inside a package dist deploys as an empty folder although the error text says submodules are not supported; only symlinks are refused (TILR-26).

## What Changes

- **BREAKING** A submodule (gitlink) inside a package dist is refused with `unsupported` before anything is deployed, like a symlink.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

Shared requirement: dist "Deploys copy the committed tree" is also modified by `deploy-ignores-git-attributes` (TILR-46); each delta changes only its own part of the current text. The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `dist`: "Deploys copy the committed tree".

## Impact

Deploy code paths (`workspace.rs`), README "Changed in 0.6". Tracker: TILR-26. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
