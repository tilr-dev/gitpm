# Proposal: Package errors name the package

## Why

Several refusals do not name the package although spec `errors` "Errors name the package" requires it: "upstream history diverged…", "rollback manifest mismatch", "plugin id changed upstream" and the unpublished-local-work refusal of `update all` (TILR-50; the moved-tag and retracted-release messages are gone with `no-tag-immutability`).

## What Changes

- Every library error about a package names its ID; the CLI adds nothing; FFI `PmError` carries `id`.

The deltas are reconciled against the current specs after TILR-39; same-version commits remain valid updates.

## Capabilities

### Modified Capabilities
- `errors`: "Errors name the package".
- `updates`: remaining branch and local-work refusals name the package.
- `ffi-bindings`: "Errors keep kind and message".

## Impact

update/rollback messages (`workspace.rs`), `error.rs`, FFI `PmError`, `updates` scenarios. Tracker: TILR-50. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
