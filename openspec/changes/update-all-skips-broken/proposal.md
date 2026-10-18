# Proposal: `update all` skips broken consumers and explains every kept-back package

## Why

The owner decided on 2026-10-18 (decision D8, TILR-37) that `update all` follows the dnf model: it updates what is healthy, skips broken consumers and reports why, never reinstalls a dependency removed with `--force`, and never fails the whole run because of a broken state. Today `update all` makes every package a solver root (review finding G2#4): after a forced uninstall it fails with `dependency_conflict` without a catalog and silently reinstalls the removed dependency with one. `HeldBack` has no reason, so the CLI prints "dependency constraints" even when the cause is a fork release without `replaces` (G2#3) or a missing dependency without a catalog (X22).

## What Changes

- `update all` solves only healthy packages, as install/update use healthy roots: a broken consumer (a direct dependency unsatisfied now, as `list` reports it) stays at its installed revision and is listed in `UpdateAllPlan.unchanged` with the new `UnchangedReason::BrokenDependency { dependency, problem }` (`dependency DEP is not installed`); its unsatisfied dependencies stay out of the solve, so a force-removed dependency is never reinstalled for it. CLI: `app skipped at V (dependency DEP is not installed)`. `list`/`check` keep reporting it unsatisfied.
- An inactive (disabled) consumer takes a disabled provider of a matching version as given in `update all`, as `update ID` already does for a disabled package.
- **BREAKING** `HeldBack` gains `reason: HeldBackReason` (`Constraints` "dependency constraints", `MissingDependency { dependency, requirement }` "needs DEP REQ, which is not installed and no catalog is configured", `NoLongerReplaces { original }` "no longer replaces ORIGINAL"; `Display` gives the message). A diverged branch stays the existing `UnchangedReason::Diverged`, not a kept-back reason. `UnchangedReason` gains `Display` too. The CLI prints the reason from the plan.
- **BREAKING** CLI: `ID updated A → B (latest L kept back by dependency constraints)` becomes `ID updated A → B (latest L kept back: REASON)`; `ID kept back at V (latest L: REASON)`.
- **BREAKING** `update all` no longer fails with `dependency_conflict` on a broken state; it exits 0 and reports. Migration: read `unchanged[].reason`.
- **BREAKING** FFI: `HeldBack` gains `reason` (`HeldBackReason` enum) and `message`; `Unchanged` gains `message`; `UnchangedReason` gains `brokenDependency(dependency, problem)`.
- Unchanged: a single `update ID` that would need a missing dependency without a catalog still fails with `dependency_conflict`.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable` (all archived 2026-10-18).

## Capabilities

### Modified Capabilities
- `dependencies`: "Broken consumers are reported, not enforced"; "Disabled packages and dependencies" (`update all` scenarios).
- `updates`: "update all never removes or downgrades".
- `cli`: "One output line per affected package".
- `ffi-bindings`: new "Kept-back reasons in records"; "Records carry the recorded mode" (`brokenDependency`).
- `fork-replacement`: "Active fork rules" (kept-back reason).

## Out of scope

- Repairing broken consumers automatically (the user reinstalls or forces).
- Reasons for single-package `update` failures (they stay errors with kinds).
- Removal and restore rules for broken consumers (later changes).

## Impact

`deps.rs` (`broken_dependencies`), `upgrade.rs` (broken consumers out of the solve, inactive consumers), `workspace.rs` (`HeldBack.reason`, `HeldBackReason`, `UnchangedReason::BrokenDependency`, reasons in `prepare_update_all`), CLI `update all` printing, FFI `HeldBack`/`Unchanged`/`UnchangedReason`, README `update all` section and "Changed in 0.6", Dagger cases in `uc_071` for forced removal followed by `update all` and a new dependency without a catalog.
