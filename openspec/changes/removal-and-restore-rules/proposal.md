# Proposal: One rule for removing replacements, a way out for missing clones, uninstall bound to its plan

## Why

Owner decisions of 2026-10-18 (D14, D16; TILR-58, 56, 60, 55) and review 2 findings: disabling an active fork is refused even when no consumer is affected while uninstalling or forgetting it succeeds (X3, G2#9), and `has_dependents` then names the replaced originals instead of consumers; `replace X --back` fails with `not_installed` once the original was uninstalled, and `replace --with` does not say whether ORIGINAL must be installed (G2#8); a recorded package whose clone was deleted can only be dropped with an undocumented `forget`, while the CLI keeps advising a `sync-dist` that fails forever (G1#1); the CLI confirmation of `uninstall` is not bound to the plan, so work created while the prompt waits is deleted unseen (G1#10), and the dependents refusal does not list the lost work (G3#23).

## What Changes

- **BREAKING** One rule for `disable`, `forget` and `uninstall` of an active replacing package: each drops the binding and redeploys ORIGINAL when active; `has_dependents` only when some consumer would break afterwards (ORIGINAL missing, disabled or of a non-matching version), and `dependents` lists only those consumers (D14, TILR-58). `HasDependents.replacing` is removed (migration: none; the refusal it described no longer exists).
- Reactivating ORIGINAL (dropping a binding by `disable`/`forget`/`uninstall` of the fork, or `replace --back`) checks ORIGINAL's direct dependencies like `enable` (`dependency_conflict`; forceable for the removals) (TILR-58).
- **BREAKING** `replace ORIGINAL --with FORK` requires ORIGINAL to be installed (`not_installed`); `replace ORIGINAL --back` with ORIGINAL absent drops the binding (`has_dependents` unless `--force` when consumers break) and prints `(ORIGINAL is not installed; broken: …)`; its `ReplacementPlan` has no `installed_version` and an empty `version` (TILR-56).
- `uninstall`/`forget` of a recorded package whose clone is gone drop the record and deployed folder (no confirmation, no `forgotten` record); the "dist not deployed" warning names uninstall/forget instead of `sync-dist` (D16, TILR-60).
- **BREAKING** `Manager::uninstall(id, confirmed: &[LostWork], force)` replaces `uninstall(id, confirm)` and `uninstall_with(id, confirm, force)` (both deleted; migration: pass the plan's `lost_work` as `confirmed`) and refuses with `local_changes` when lost work grew beyond `confirmed`; `HasDependents.lost_work` carries the plan's lost work; `UninstallPlan::refusal()` returns the `has_dependents` refusal so the CLI no longer calls `uninstall` to obtain it; FFI `uninstall(confirmed, force)`, `PmError.HasDependents.lostWork` and free function `uninstallRefusal(plan)` (TILR-55).
- **BREAKING** `forget` returns `ForgetResult { clone_kept, dist }` (FFI the same record); the CLI says `forgotten (no clone; record and dist removed)` when the clone was gone. Migration: read `.dist`.
- **BREAKING** A replacing package is always enabled: a `state.json` binding ORIGINAL to a disabled package is `invalid_data`; `ReplacementPlan.fork_enabled` and the "stays installed (disabled)" switch-back ending are removed. Migration: re-create a hand-edited state.
- `local_changes` messages name no flag: uninstall says "confirm the listed lost work to remove it", other refusals "commit or discard the local changes first".
- **BREAKING** `switch_back(original, confirm, force)` (FFI `switchBack(original, confirm, force)`): consumers a missing or disabled ORIGINAL does not serve refuse with `has_dependents` unless `force`; `confirm` covers only incompatible versions. `UninstallPlan.dependency_conflict` (FFI `dependencyConflict`) carries the reactivation refusal, returned by `refusal()` before any lost-work question. `SkippedDist.clone_missing` (FFI `cloneMissing`) decides the CLI's uninstall/forget advice. Migration: pass `force`.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `dependencies` ("Dependents guard removal", "enable needs active dependencies"), `fork-replacement` ("Active fork rules", "Replacement is explicit and global", "Switching back", "Replacement plans carry what the output needs"), `dist` ("Enable and disable deploy and remove", "Forget, restore and uninstall settle dist", "Undeployable packages do not block others"), `install-uninstall` (new "Removing a package whose clone is gone"; "Three removal levels", "Uninstall is planned and confirmed", "CLI confirmation flow for uninstall"), `errors` ("Stable error kinds", "Display is the complete message": `HasDependents.lost_work`; no new error kind), `ffi-bindings` ("Manager surface", "Errors keep kind and message"), `cli` ("Errors and exit codes").

## Out of scope

- Re-cloning a missing clone from its source (`sync-dist` stays a pure reconciliation).
- Restoring a binding when a disabled fork is enabled again (the user replaces again).

## Impact

`workspace.rs` (guard, disable/forget/uninstall with bindings, switch back, missing clones, uninstall signature), `model.rs` (binding to a disabled package refused), `error.rs` (`HasDependents.lost_work`), CLI uninstall flow and warnings, FFI `uninstall`, README removal section and "Changed in 0.6".
