# Proposal: Group the catalog commands under `gitpm catalog`

## Why

The Dagger integration suite (`~/projects/tilr/pm-dagger`) drives the stand CLI `pm` (`~/projects/tilr/temp-plugin-manager/manager/src/main.rs`), and the owner decided not to port that adapter: every scenario must run against the real `gitpm` binary (decision recorded in the task brief of 2026-10-18, change `dagger-integration-tests`). A comparison of the stand's command table with `crates/gitpm-cli/src/main.rs` (`COMMANDS`, 0.5.0-dev at `0b146cf`) shows that the generic stand commands already exist in gitpm under the same flat names: `catalog-update`, `migrate-catalog`, `register`, `info`, `catalog`, `auth check`, `remote`, `commit`, `init`/`create`. What is left is the shape the owner asked for: the catalog commands as one group, `gitpm catalog update|migrate|register|show`, so the suite (and users) address catalog work with one noun instead of four unrelated verbs. The stand-only commands are either Tilr infrastructure (`sync`, `catalog-update --publish`) or were removed from gitpm on purpose (`namespace`, default namespace config: change `2026-10-16-gitpm-cli`, "He did not want a config file or a namespace command"; spec `package-ids` "No default namespace").

## What Changes

- New command group `gitpm catalog SUBCOMMAND`:
  - `catalog show` — the bounded text overview (today `info`);
  - `catalog links` — the JSON directory links of one catalog level (today bare `catalog`);
  - `catalog update [--sources FILE]` — generate/refresh a local catalog from Git release tags (today `catalog-update`);
  - `catalog migrate` — split a local v1 catalog into the v2 tree (today `migrate-catalog`);
  - `catalog register GIT_SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` — add one committed version (today `register`).
- `info`, `catalog` (without a subcommand), `catalog-update`, `migrate-catalog` and `register` stay as deprecated aliases: same behavior, same stdout, plus one `Warning: … is deprecated; use gitpm catalog …` line on stderr. Not breaking in this change; their removal is a later change marked **BREAKING**.
- Help: a `Catalog` section listing the subcommands; `gitpm catalog --help` prints the whole group, `gitpm catalog SUB --help` one block.
- `auth check GIT_URL` and `remote REPO URL` are kept unchanged (already generic, already in gitpm, already specified in `git-safety` and `authoring`).
- README: command table and a "Changed in 0.6" entry for the new names and the deprecations.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli`: adds the `catalog` command group, the deprecated aliases and the group's help and error messages.
- `catalog`: the overview limit line names `gitpm catalog show` instead of `info`.

## Out of scope

- `sync` / `catalog-update --publish` (R2 upload and Cloudflare purge, default `pkg.tilr.dev`): Tilr hosting infrastructure; gitpm is host-agnostic and only writes local catalog folders (spec `cli` "Nothing is built, executed or uploaded").
- A `namespace` command, a default namespace, `GITPM_NAMESPACE` or a config file: removed on purpose in 0.3/0.4 (see Why); `init` keeps `NAMESPACE/NAME` and `fork` keeps `--ns`.
- The stand's `info` defaults (`PM_REMOTE_CATALOG`, `PM_PUBLIC_URL`, fallback `https://pkg.tilr.dev`): gitpm has no built-in catalog (spec `catalog` "One catalog per workspace").
- `create` as a separate command: it already is an alias of `init` in gitpm.
- Removing the deprecated names, any library or FFI API change, any change to catalog formats.

## Impact

`crates/gitpm-cli/src/main.rs` (dispatch, option validation, help), `crates/gitpm-cli/tests/` (new tests for the group and the aliases), `README.md`. No library, FFI or file-format change. The `dagger-integration-tests` change depends on this one: its scenarios use the new names.
