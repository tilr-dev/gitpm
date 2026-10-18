# Proposal: `update all` skips broken consumers and explains every kept-back package

## Why

The owner decided on 2026-10-18 (decision D8, TILR-37) that `update all` follows the dnf model: it updates what is healthy, skips broken consumers and reports why, never reinstalls a dependency removed with `--force`, and never fails the whole run because of a broken state. Today `update all` makes every package a solver root (review finding G2#4): after a forced uninstall it fails with `dependency_conflict` without a catalog and silently reinstalls the removed dependency with one. `HeldBack` has no reason, so the CLI prints "dependency constraints" even when the cause is a fork release without `replaces` (G2#3) or a missing dependency without a catalog (X22).

## What Changes

- `update all` solves only healthy packages (as install/update already do); a consumer with unsatisfied dependencies is kept back with reason `dependency DEP is not installed`; force-removed dependencies are never reinstalled.
- `HeldBack.reason` in the library (and FFI `keptBack[].reason`): `dependency constraints`, `needs DEP REQ, which is not installed and no catalog is configured`, `dependency DEP is not installed`, `no longer replaces ORIGINAL`, `history of BRANCH diverged`. The CLI prints the reason from the plan.
- **BREAKING** `update all` no longer fails with `dependency_conflict` on a broken state; it exits 0 and reports.
- Unchanged: a single `update ID` that would need a missing dependency without a catalog still fails with `dependency_conflict`.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`.

## Capabilities

### Modified Capabilities
- `dependencies`: "Broken consumers are reported, not enforced".
- `updates`: "update all never removes or downgrades".
- `cli`: "One output line per affected package".
- `ffi-bindings`: "Records mirror library results".
- `fork-replacement`: "Active fork rules" (kept-back reason).

## Out of scope

- Repairing broken consumers automatically (the user reinstalls or forces).
- Reasons for single-package `update` failures (they stay errors with kinds).

## Impact

`upgrade.rs` (healthy roots, reasons), `workspace.rs` (`HeldBack`), CLI `update all` printing, FFI `HeldBack`, README `update all` section and "Changed in 0.6", Dagger cases for forced removal followed by `update all`.
