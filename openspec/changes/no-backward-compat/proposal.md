# Proposal: No backward compatibility before 1.0

## Why

The owner decided on 2026-10-18 (decision D1, TILR-38) that gitpm keeps no backward compatibility before 1.0. The code still carries several compatibility paths that the specs describe as current behavior: legacy unscoped IDs and `vVERSION` tags (spec `package-ids`), state.json format 1 read and rewritten as format 2 (`workspace-layout` "State format versions"), v1 monolithic catalogs and `migrate-catalog` (`catalog` "v1 catalogs stay readable"), the CLI alias `create` for `init`, and metadata read from the clone for records of gitpm 0.4 (`plugin-metadata` "Metadata in state"). `gitpm-ffi` keeps its own version 0.1.0 (review finding G3#12).

## What Changes

- **BREAKING** Package IDs are always `namespace/name`; unscoped IDs fail with `invalid_id`. `update all` can no longer collide with an ID `all`. Migration: re-create the package under a namespace.
- **BREAKING** Release tags are only `ID@VERSION`; `vVERSION` tags are not releases (single- and multi-package repositories). Migration: tag `ID@VERSION`.
- **BREAKING** `state.json` other than the current format (no `format`, or a newer one) is refused with `unsupported` and never rewritten. Rule: the format is bumped whenever an older gitpm would misread the content; there is no migration. Migration: re-create the workspace.
- **BREAKING** v1 catalogs and `migrate-catalog` are removed; `legacy/<id>/` and releases without file hashes are gone. Migration: regenerate the catalog with `catalog-update`.
- **BREAKING** CLI alias `create` is removed (`init` only).
- `list` reports metadata only from the state record (no clone fallback for 0.4 records).
- `gitpm-ffi` carries the library version (release-please updates it with `gitpm`/`gitpm-cli`); the FFI promises no stability before 1.0.

Depends on: none (first of the 2026-10-18 batch: 1 → 2 → 3 → 6 → 7 → 8 → 11 → 5 → 4 → 9 → 10 → 12). Rebase: `cli-catalog-commands` modifies cli "Command set", "Commands that need a catalog say so" and "Help", and `catalog-metadata-index` modifies catalog "Catalog v2 is a tree of static files"; whichever is archived second rebases its block.

## Capabilities

### Modified Capabilities
- `package-ids`, `multi-package-repos`, `workspace-layout`, `catalog`, `cli`, `authoring`, `errors`, `dist`, `plugin-metadata`, `release-process`, `ffi-bindings`.

## Out of scope

- Any migration tool, alias or deprecation window.
- The `PM_*` variables (already ignored).
- The Plugin → Package rename of literal names (TILR-43).

## Impact

`identity.rs`, `model.rs` (ID and state format), `releases.rs`, `catalog.rs` (v1 reader, `migrate`, legacy paths), `check.rs`/`model.rs` metadata fallback, CLI command table and help, `crates/gitpm-ffi/Cargo.toml` + `release-please-config.json` extra-files, README "Changed in 0.6", Dagger cases using legacy IDs or `v*` tags. Tracker: TILR-38.
