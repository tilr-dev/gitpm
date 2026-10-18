# Proposal: Group the catalog commands under `gitpm catalog`

## Why

The catalog operations are four unrelated top-level verbs today (`info`, `catalog`, `catalog-update`, `register`, plus `migrate-catalog`), checked against `crates/gitpm-cli/src/main.rs` (0.5.0-dev at `0b146cf`). The owner asked for one noun, `gitpm catalog show|links|update|register`, when the Dagger suite was moved into this repository (2026-10-18, change `2026-10-18-dagger-integration-tests`; tracker TILR-8). The owner also decided that gitpm keeps no backward compatibility before 1.0 (decision D1 of 2026-10-18, TILR-38): no CLI aliases, so the old names are removed in the same change. The stand-only commands stay out: `sync`/`catalog-update --publish` are Tilr hosting, `namespace` was removed on purpose (change `2026-10-16-gitpm-cli`; spec `package-ids` "No default namespace").

## What Changes

- **BREAKING** New command group `gitpm catalog SUBCOMMAND`, replacing the flat names (no aliases; migration: the rename table in README "Changed in 0.6"):
  - `catalog show` — the bounded text overview (was `info`);
  - `catalog links` — the JSON directory links of one catalog level (was bare `catalog`);
  - `catalog update [--sources FILE]` — generate/refresh a local catalog from Git release tags (was `catalog-update`);
  - `catalog register GIT_SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` — add one committed version (was `register`).
- **BREAKING** `info`, `catalog-update` and `register` become unknown commands; bare `gitpm catalog` without a subcommand fails naming the subcommands.
- `migrate-catalog` is not moved into the group: it goes away with v1 catalog support (TILR-38); until then it keeps its name.
- Help: a `Catalog` section listing the subcommands; `gitpm catalog --help` prints the whole group, `gitpm catalog SUB --help` one block.
- Thin CLI (decision D2, TILR-29..33): the library message for writing to an HTTP(S) catalog stops naming a command ("catalog-update requires a local output catalog" becomes neutral); the CLI adds the command hint.
- Dagger suite: `uc_076` (catalog group, unknown subcommand, old names unknown) and the `gitpmtest.COMMANDS` remap to the new spellings, carried over from `2026-10-18-dagger-integration-tests` (its tasks 0.1 and 5.5).
- README: command table and a "Changed in 0.6" entry with the renames.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli`: the `catalog` command group, its help and errors; MODIFIED "Command set", "Commands that need a catalog say so", "Option validation", "Help", "Nothing is built, executed or uploaded".
- `catalog`: MODIFIED "Bounded overview" (the limit line names `gitpm catalog show`).
- `plugin-metadata`: MODIFIED "Metadata in the catalog" (`catalog show` instead of `info`).

## Out of scope

- Deprecated aliases or a transition window (D1: no backward compatibility before 1.0).
- `sync` / `catalog-update --publish` (R2 upload and Cloudflare purge): Tilr hosting infrastructure; gitpm only writes local catalog folders (spec `cli` "Nothing is built, executed or uploaded").
- A `namespace` command, a default namespace, `GITPM_NAMESPACE` or a config file (see Why).
- Removing `migrate-catalog` and v1 catalogs (TILR-38).
- Library or FFI API changes beyond the neutral message; catalog format changes.

## Impact

`crates/gitpm-cli/src/main.rs` (dispatch, option validation, help, error hints), `crates/gitpm/src/catalog.rs` (one message), `crates/gitpm-cli/tests/`, `itests/` (`gitpmtest.COMMANDS`, `uc_001`/`uc_063` help assertions, new `uc_076`), README. Spec requirements that still name the old commands in examples (catalog, cli, plugin-metadata) get MODIFIED deltas before archive (task 3.2).
