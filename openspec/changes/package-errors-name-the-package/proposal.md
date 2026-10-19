# Proposal: Package errors name the package

## Why

Several refusals do not name the package although spec `errors` "Errors name the package" requires it: "upstream changed without a version bump", "upstream history diverged…", "rollback manifest mismatch", "plugin id changed upstream" and the unpublished-local-work refusal of `update all` (TILR-50; the moved-tag and retracted-release messages are gone with `no-tag-immutability`).

## What Changes

- Every library error about a package names its ID; the CLI adds nothing; FFI `PmError` carries `id`.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `errors`: "Errors name the package".
- `ffi-bindings`: "Errors keep kind and message" (its field list also carries `NotDeployed.skipped` and `IncompatibleHost` as earlier changes of the chain leave them).

## Impact

update/rollback messages (`workspace.rs`), `error.rs`, FFI `PmError`, `updates` scenarios. Tracker: TILR-50. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
