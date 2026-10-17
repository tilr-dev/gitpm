# Proposal: Extract the embeddable library and UniFFI wrapper

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

The owner wanted a crate the Tilr macOS app can import; the app draws its own UI and talks to the crate through an API ("no web part"). Decisions recorded in Claude session 2026-10-15 16:39 (S1): the library prints nothing, reads no terminal or environment, returns typed errors with stable kinds, confines git/curl to one module, and asks for confirmation through plan objects. Uninstall with local work: "if he confirms uninstall, it deletes".

## What Changes

- New repository `~/projects/tilr/plugin-manager` with crates `tilr-plugin-manager` (library) and `tilr-plugin-manager-ffi` (UniFFI, Swift), `scripts/build-apple.sh`.
- Workspace transactions, install/update/rollback, fork, catalog v2, check, auth moved into the library.
- `uninstall_plan` lists the lost work (modified, untracked, ignored, staged, unpublished commits/branches/tags, stash) and the state entry; `uninstall(confirm)` deletes only what the workspace owns.
- Per-module unit tests with shared `test_support.rs`; hermetic Git config; known defects kept visible as `#[ignore = "known defect: …"]`.

## Capabilities

### Modified Capabilities
- `install-uninstall`
- `errors`
- `ffi-bindings`
- `git-safety`

## Out of scope

- Dependencies and replacement (not supported in this first version).
- Mac app integration commits (separate repository).

## Impact

Implemented by: [`9d21a20`](https://github.com/tilr-dev/gitpm/commit/9d21a20) Initial plugin manager library and UniFFI wrapper (2026-10-15 20:44); [`993a1db`](https://github.com/tilr-dev/gitpm/commit/993a1db) Unit tests for library and FFI modules (2026-10-15 21:06).
