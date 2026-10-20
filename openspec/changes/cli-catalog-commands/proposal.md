# Proposal: Group the catalog commands under `gitpm catalog`

## Why

The catalog operations were four unrelated top-level verbs: `info`, `catalog`, `catalog-update` and `register`. The owner asked for one noun, `gitpm catalog show|links|update|register`, when the Dagger suite was moved into this repository (2026-10-18, change `2026-10-18-dagger-integration-tests`; tracker TILR-8), and decided on variant (c): rename, breaking, in one step (owner decision of 2026-10-18 on TILR-8; no backward compatibility before 1.0, TILR-38). The stand-only commands stay out: `sync`/`catalog-update --publish` are Tilr hosting, `namespace` was removed on purpose (change `2026-10-16-gitpm-cli`; spec `package-ids` "No default namespace").

## What Changes

- **BREAKING** New command group `gitpm catalog SUBCOMMAND`, replacing the flat names (migration: the rename table in README "Changed in 0.6"):
  - `catalog show` — the bounded text overview (was `info`);
  - `catalog links` — the JSON directory links of one catalog level (was bare `catalog`);
  - `catalog update [--sources FILE]` — generate/refresh a local catalog from Git release tags (was `catalog-update`);
  - `catalog register GIT_SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` — add one committed version (was `register`).
- **BREAKING** `info`, `catalog-update` and `register` become unknown commands; bare `gitpm catalog` without a subcommand fails naming the subcommands.
- Help: a `Catalog` section listing the subcommands; `gitpm catalog --help` prints the whole group, `gitpm catalog SUB --help` one block.
- Hints: the library message for writing to an HTTP(S) catalog is already neutral (`thin-cli-boundary`); the CLI hint names `gitpm catalog update`.
- Dagger suite: `uc_076` (catalog group, unknown subcommand, old names unknown) and the `gitpmtest.COMMANDS` remap to the new spellings, carried over from `2026-10-18-dagger-integration-tests` (its tasks 0.1 and 5.5).
- README: command table and a "Changed in 0.6" entry with the renames.

The preceding catalog and CLI changes are integrated. `catalog-metadata-index` builds on this change.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli`: catalog group dispatch, help, errors and command names in output scenarios.
- `catalog`: command names in browsing, generation, registration and catalog safety scenarios.
- `plugin-metadata`, `package-ids`, `multi-package-repos`, `ffi-bindings`: CLI command names in examples; API names and behavior stay unchanged.

The MODIFIED deltas cover the 26 requirements authorized by Dan in TILR-8 comments #4174, #4178 and #4180; their existing behavior is preserved.

## Out of scope

- `sync` / `catalog-update --publish` (R2 upload and Cloudflare purge): Tilr hosting infrastructure; gitpm only writes local catalog folders (spec `catalog` "Nothing is built, executed or uploaded").
- A `namespace` command, a default namespace, `GITPM_NAMESPACE` or a config file (see Why).
- Library or FFI API changes; catalog format changes (`catalog-metadata-index`).

## Impact

`crates/gitpm-cli/src/main.rs` (dispatch, option validation, help, error hints), `crates/gitpm-cli/tests/`, `itests/` (`gitpmtest.COMMANDS`, `uc_001`/`uc_063` help assertions, new `uc_076`), README. Spec requirements that still name the old commands in examples (catalog, cli, plugin-metadata and the requirements the chain added, for example catalog "Verified catalogs") get MODIFIED deltas before archive (task 3.2). Tracker: TILR-8.

## Implementation

Implementing commit: `9290151` (TILR-8, catalog group dispatch, documentation and integration scenarios). The FFI parity test follows the renamed command keys; FFI functions are unchanged.
