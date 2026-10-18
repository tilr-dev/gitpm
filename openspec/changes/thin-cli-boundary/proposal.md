# Proposal: Thin CLI boundary

## Why

The owner decided on 2026-10-18 (decision D2, TILR-29, 31, 32, 33, 22, 48) that the CLI only parses argv and environment, calls the library, prints results, asks TTY prompts and adds command hints per error kind. Review 2 found logic that lives only in the CLI today: a nonexistent `--catalog` path silently becomes "no catalog"; `list` builds and filters its JSON itself and takes the workspace lock; `fork --replace` re-plans and replaces after an incompatible-replacement failure; replace and `update all` lines read state the result does not carry; the CLI decides that `sync-dist` failed; FFI `InstallResult`/`RevisionChange` drop `skipped`/`warnings` (G3#13); library messages name CLI commands and R2 (G3#2); prompts use two streams (G3#25).

## What Changes

- **BREAKING** A nonexistent local catalog location fails in the library (`invalid_argument`) instead of counting as no catalog; `open_with` validates Options before creating anything.
- `list` is read-only like `check` (no create, lock or recovery); the combined view and `--provides` filter move into the library (`list_with`).
- `fork --replace` returns a pending `ReplacementPlan` when the replace step needs confirmation; `fork_replace_resume` applies it. `ReplacementPlan` gains `installed_version` and `fork_enabled`; `commit_package` returns `{package, version, tag}`; `UpdateAllPlan` gains `unchanged`.
- Every mutating result carries its dist report; **BREAKING** `sync-dist` leaving packages undeployed is the library error `not_deployed` (new kind).
- **BREAKING** FFI: free functions `list`, `check`, `readState`, `catalogLinks`; `PluginManager.check` removed; records carry `skipped`, `warnings`, dist report, `unchanged`, plan fields; `forkReplaceResume`.
- Library messages are neutral (no command names, no R2); the CLI adds `Hint:` lines by kind; prompts go to stderr; the CLI no longer checks domain combinations (`--channel dev --ref`), and `init NAME` without namespace is the library's `invalid_id`.
- Spec moves out of `cli`: "Nothing is built…" → catalog; dist warnings → dist; sync-dist exit → dist; prompt refusals → install-uninstall/fork-replacement; "one output line" limited to workspace-mutating commands; CLI/FFI parity requirement in ffi-bindings.

Depends on: `no-backward-compat`. Rebase: `cli-catalog-commands` and `fork-repository-without-workspace` modify cli "Option validation"; `cli-catalog-commands` also modifies "Commands that need a catalog say so" and "Nothing is built, executed or uploaded" (removed here; its scenario lives in catalog).

## Capabilities

### Modified Capabilities
- `cli`, `catalog`, `dist`, `errors`, `install-uninstall`, `multi-package-repos`, `fork-replacement`, `authoring`, `package-ids`, `updates`, `transactions-locking`, `workspace-layout`, `ffi-bindings`, `plugin-metadata`.

## Out of scope

- New CLI commands or output formats beyond the hint lines.
- Host (Tilr) UI for confirmations.
- Checking from the recorded origin (`check-via-origin`).

## Impact

`workspace.rs` (results, `open_with`, `list_with`, `fork_replace_resume`, `sync_dist`), `check.rs`, `scaffold.rs`, `catalog.rs` messages, `error.rs` (`NotDeployed`), `crates/gitpm-cli/src/main.rs` (printing only, hints), `crates/gitpm-ffi/src/lib.rs`, README "Changed in 0.6" and FFI section.
