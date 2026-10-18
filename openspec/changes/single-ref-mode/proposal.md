# Proposal: One `--ref` with recorded modes instead of `--channel`

## Why

The owner decided on 2026-10-18 (decision D6, TILR-40) to replace `--channel stable|dev` with one `--ref`: a tag or commit pins, a branch follows, `--dev` means the default branch, and the mode is recorded in state. Today install records neither channel nor ref (review finding G2#1, G1#2): a plain `update ID` of a dev install fails with `version_conflict`, `update all` treats a dev commit as a candidate, and `install --ref BRANCH` fails on a fresh clone because only `origin/BRANCH` exists (TILR-47). Restore differs between own and shared clones: an own clone registers HEAD as found, drops `catalog` and forces `enabled: true` (G1#3; decision D15, TILR-59).

## What Changes

- **BREAKING** `--channel` is removed (CLI) and the FFI `Channel` enum becomes `Target` (`Release`, `Ref`, `Dev`, and `Recorded` for update). `install --ref TAG|COMMIT` pins, `--ref BRANCH` follows that branch, `--dev` follows the origin's default branch (its name is recorded).
- **BREAKING** The record gains `mode` (omitted = release, `{"pinned": REF}`, `{"branch": NAME}`); still `state.json` format 3: no other bump, because format 3 is introduced by `no-backward-compat` in the same release 0.6.0 and no released gitpm writes format 3 without `mode`.
- `update ID` follows the mode (pinned: unchanged with reason `pinned`; branch: HEAD by commit, diverged history refused); `--ref`/`--dev` switch the mode; new `update ID --release` returns to release mode. `update all` honors every mode and takes no target.
- `install --ref BRANCH` resolves against the origin (TILR-47).
- **BREAKING** Restore unified: every forget keeps a `forgotten` record (own clones too); restore re-records it with revisions, `catalog`, `enabled` and mode; release/pinned at the recorded commit, branch mode at HEAD as found; lost work, unpublished commits included, is refused (TILR-59).
- `check` shows `pinned` rows and compares branch-mode packages with the branch HEAD.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`. `cli-catalog-commands` and `fork-repository-without-workspace` (after the chain) already build their cli "Command set"/"Option validation" on this change.

## Capabilities

### Modified Capabilities
- `install-uninstall`, `updates`, `workspace-layout`, `cli`, `ffi-bindings` ("Manager surface"; new "Records carry the recorded mode"), `multi-package-repos`, `catalog` (check rows), `dist` ("Read-only commands never change dist").

## Out of scope

- Live, uncommitted dev working trees (TILR-35 research).
- Pinning catalog installs: `install-catalog --version V` stays release mode.
- A migration of older workspaces.

## Impact

`model.rs` (`Mode`, `State` record shape (format 3), forgotten records for own clones), `workspace.rs` (install/update targets, restore), `upgrade.rs` (modes in `update all`), `check.rs`, CLI option parsing and lines (`pinned at`), FFI `Target`, README "Changed in 0.6", Dagger cases using `--channel`.
