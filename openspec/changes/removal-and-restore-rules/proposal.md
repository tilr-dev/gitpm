# Proposal: One rule for removing replacements, a way out for missing clones, uninstall bound to its plan

## Why

Owner decisions of 2026-10-18 (D14, D16; TILR-58, 56, 60, 55) and review 2 findings: disabling an active fork is refused even when no consumer is affected while uninstalling or forgetting it succeeds (X3, G2#9), and `has_dependents` then names the replaced originals instead of consumers; `replace X --back` fails with `not_installed` once the original was uninstalled, and `replace --with` does not say whether ORIGINAL must be installed (G2#8); a recorded package whose clone was deleted can only be dropped with an undocumented `forget`, while the CLI keeps advising a `sync-dist` that fails forever (G1#1); the CLI confirmation of `uninstall` is not bound to the plan, so work created while the prompt waits is deleted unseen (G1#10), and the dependents refusal does not list the lost work (G3#23).

## What Changes

- **BREAKING** One rule for `disable`, `forget` and `uninstall` of an active replacing package: each drops the binding and redeploys ORIGINAL when active; `has_dependents` only when some consumer would break afterwards, and `dependents` lists only those consumers (D14, TILR-58).
- **BREAKING** `replace ORIGINAL --with FORK` requires ORIGINAL to be installed (`not_installed`); `replace ORIGINAL --back` with ORIGINAL absent drops the binding (`has_dependents` unless `--force` when consumers break) (TILR-56).
- `uninstall`/`forget` of a recorded package whose clone is gone drop the record and deployed folder (no confirmation, no `forgotten` record); the "dist not deployed" warning names uninstall/forget instead of `sync-dist` (D16, TILR-60).
- **BREAKING** `uninstall(id, confirmed: &[LostWork], force)` refuses with `local_changes` when lost work grew beyond `confirmed`; `has_dependents` from uninstall carries `lost_work` too; FFI `uninstall(confirmed, force)` (TILR-55).

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`.

## Capabilities

### Modified Capabilities
- `dependencies` ("Dependents guard removal"), `fork-replacement` ("Active fork rules", "Replacement is explicit and global", "Switching back"), `dist` ("Enable and disable deploy and remove", "Undeployable packages do not block others"), `install-uninstall` (new "Removing a package whose clone is gone"; "Uninstall is planned and confirmed", "CLI confirmation flow for uninstall"), `errors`, `ffi-bindings`, `cli` ("Errors and exit codes").

## Out of scope

- Re-cloning a missing clone from its source (`sync-dist` stays a pure reconciliation).
- Restoring a binding when a disabled fork is enabled again (the user replaces again).

## Impact

`workspace.rs` (guard, disable/forget/uninstall with bindings, switch back, missing clones, uninstall signature), `clones.rs`, `error.rs` (`HasDependents.lost_work`), CLI uninstall flow and warnings, FFI `uninstall`, README removal section and "Changed in 0.6".
