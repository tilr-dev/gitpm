# Proposal: Refuse a derived Git identity

## Why

The identity check accepts an identity git derives from the login and host name, so `commit` succeeds with an invented identity, and a refusal fails with `git` instead of `invalid_argument` (review 2, G2#13, TILR-49).

## What Changes

- **BREAKING** One identity check via `git -c user.useConfigOnly=true var`; a missing identity fails with `invalid_argument` (`commit`, `init` into an existing repository, adding packages to an existing fork).

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `authoring`: "Git identity is never invented in existing repositories".
- `multi-package-repos`: "Adding packages to an existing fork".

## Impact

`scaffold.rs` (identity), README rules and "Changed in 0.6". Tracker: TILR-49. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
