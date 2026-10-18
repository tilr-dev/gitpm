# Proposal: Move the Dagger integration suite into gitpm and run it against `gitpm`

## Why

gitpm's black-box suite lives outside the repository, in `~/projects/tilr/pm-dagger` (Python Dagger module, engine v0.21.10, 41 fixtures, 67 `uc_NNN.py` files, 506 `@case`s), and it does not test the `gitpm` binary: it drives the stand CLI `pm` from `~/projects/tilr/temp-plugin-manager` (env `PM_*`, Russian output, `namespace`/`sync` commands). The stand no longer builds against gitpm `dev` (it calls `identity::resolve_namespace` and `scaffold::create_namespaced`, removed in 0.4), and its last recorded run (`results/20261016T172214`: 911 tests, 909 pass, `uc_065` fail, `uc_022` blocked) predates 0.4 and 0.5, so nothing from 0.4–0.5 (dist active-only, `sync-dist`, `update all`, multi-package repositories, metadata, idempotent install, unified output) is covered end to end. The owner decided (2026-10-18): do not port the adapter; run every scenario against `gitpm`, from this repository. The catalog command group (change `cli-catalog-commands`, TILR-8) is not a prerequisite: until it lands the cases use the current flat names through one mapping (`gitpmtest.COMMANDS`).

## What Changes

- A Dagger module at `itests/` in this repository (`dagger.json` engine v0.21.10, Python SDK), with the helper (`gitpmtest`, renamed from `pmtest`), fixtures and scenarios, and a wrapper `./itests/dt` (default engine on pb via `dagger-pb`, `DT_ENGINE=dagger` for a local engine; `--only`, `--filter`, `--cached`, `--jobs`, `--open`). The module's source is this repository; there are no `--stand`/`--library` arguments.
- Test kinds: `rust` (every libtest of the gitpm workspace, one container each), `lint` (clippy of the workspace), `fixture`, `scenario`. The stand's `sync` kind (`scripts/test_sync.py`) is dropped.
- Every case calls `gitpm` with `GITPM_ROOT`/`GITPM_CATALOG`/`GITPM_SOURCES` and 0.4+ syntax (`init NS/NAME --into DIR`, `fork ID --ns NS --into DIR`); catalog cases use the current flat names (`info`, `catalog`, `catalog-update`, `register`, `migrate-catalog`) through `gitpmtest.COMMANDS` until `cli-catalog-commands` (TILR-8) renames them. Cases for `sync`, `namespace`, the interactive init prompt and the stand wrapper are dropped as obsolete; the per-UC mapping is in design.md.
- New cases for 0.4–0.5 behavior. The case for the catalog group (`uc_076`) and the `gitpmtest.COMMANDS` remap move to `cli-catalog-commands` (TILR-8).
- Every case is tagged with the spec requirement (and optionally scenario) it verifies; `./itests/dt coverage` writes a coverage report to `openspec/coverage/` and fails on untagged cases or unknown tags.
- Known gaps stay visible: `uc_065:oversized_streamed_catalog_response` is an expected failure until `catalog-streaming-size-limit` lands; `uc_022` stays BLOCKED on case-sensitive filesystems.

## Capabilities

### New Capabilities
- `integration-tests`: how the black-box suite is run (entry point, engines, filters, results), what it may call (only the `gitpm` binary with `GITPM_*`), how cases are tagged with spec requirements, and the coverage report.

### Modified Capabilities
- None: gitpm behavior does not change; CI gates (spec `release-process`) are unchanged.

## Out of scope

- Porting the stand adapter or keeping `PM_*` compatibility.
- `sync` / R2 / Cloudflare cases (Tilr infrastructure; gitpm never uploads).
- Porting `pm-dagger/results/` (old run reports) or the Python SDK folder (`itests/sdk/` is generated and gitignored).
- A required CI job: the suite runs on pb only for now (design D10; Dagger in GitHub CI is TILR-44).
- Cutover and deletion of `~/projects/tilr/pm-dagger` and `~/projects/tilr/temp-plugin-manager`: separate task TILR-13, needs the owner's confirmation.
- Anything Tilr-specific (hosts, slots, `plugins.json`).
- Fixing defects the suite finds: each becomes its own change.

## Impact

New `itests/` tree (Python, Dagger module), `openspec/coverage/` (generated report), `.gitignore` entries, README "Tests" section. No Rust code changes and no dependency on `cli-catalog-commands`. Runtime on pb comparable to the old suite (~2 min for ~900 tests).

Implemented on `dev` in the working tree after `8d9e2f9` (not yet committed at archive time, 2026-10-18); archived with tasks 0.1 and 5.5 (`uc_076`) moved to `cli-catalog-commands`.
