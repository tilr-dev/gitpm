# Design

## Context

`model.rs` `Installed` has no mode; `install_channel`/`update_channel` take `Channel::{Stable, Dev}`; `resolve_ref` on a fresh clone looks only at local refs, so `install --ref feature` fails (TILR-47). Own-clone restore (`workspace.rs` restore) builds a new record from the clone; shared-clone restore uses the `forgotten` record. Decisions D6 and D15.

## State field shape

```json
"plugins": {
  "acme/a": { "source": "…", "current": {…}, "enabled": true },
  "acme/b": { "…": "…", "mode": { "pinned": "acme/b@1.0.0" } },
  "acme/c": { "…": "…", "mode": { "branch": "main" } }
}
```

- `mode` is omitted in release mode (the common case keeps records short; omission is not a compatibility shim because format 2 files are refused).
- `pinned` holds the ref as the user gave it (tag name or full commit); `current.commit` is the truth, the ref is for display and `check`.
- `branch` holds the branch name. `--dev` resolves the origin's default branch at install (`git remote set-head origin --auto`) and records its name, so a later rename of the default branch does not silently switch what the package follows.
- Rust: `enum Mode { Release, Pinned(String), Branch(String) }` with `#[serde(skip_serializing_if = "Mode::is_release")]`; FFI `enum Target { Release, Ref(String), Dev, Recorded }` (install rejects `Recorded` at the type level by taking `InstallTarget` without it).
- `state.json` `"format": 3`.

## Decisions

- `update ID --release` is the way back from pinned or branch mode (alternative: `--ref` with the newest tag, rejected: it would pin again).
- A pinned package in `update ID` returns an unchanged result with reason `pinned` (exit 0), not an error: the user asked to update and the package is deliberately fixed.
- `update all` gives a branch-mode package one candidate (its branch HEAD); its manifest version takes part in the solve like a release, so dependents are still protected. Diverged branch history keeps it back with a reason (`update-all-skips-broken` adds the reason field).
- Restore keeps the record (decision D15): release/pinned check out `current.commit`; branch mode records HEAD as found when it is published. Every forget writes a `forgotten` record so own and shared clones restore the same way.
- `install-catalog --version V` stays release mode: the catalog chooses a version, later updates follow releases (`protected-catalog` limits them to catalog versions).

## Transactions and crash recovery

Mode changes are part of the same state.json write as the revision change; no new staged paths. Restore stages the record move from `forgotten` to `plugins` and the deploy in one transaction, as today for shared clones. A crash before commit leaves the forgotten record; after commit the journal rolls forward.

## Risks

- Every existing workspace must be re-created (format 3). Documented in README "Changed in 0.6".
