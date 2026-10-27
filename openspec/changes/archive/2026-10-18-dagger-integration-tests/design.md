# Design

## Context

See proposal.md (Why). Facts this design relies on:

- `~/projects/tilr/pm-dagger` (HEAD `556d8e5`): `dagger.json` (`pm-tests`, engine v0.21.10, Python SDK), `src/pm_tests/main.py` (module: build, rust/lint/sync/fixture/scenario kinds, report bundle), `itests/pmtest/` (`@fixture`, `@case`, `T` helper with `pm_ok`/`pm_err`/`git`/`release`/`snap`/`serve`), `itests/fixtures/` (9 files, 41 fixtures, source tier = git only, pm tier = uses the binary), `itests/scenarios/` (67 files `uc_001`…`uc_068`, no `uc_067`; 506 cases), `tools/report.py`, `tools/history.py`, wrapper `./dt`.
- gitpm 0.5 CLI (`crates/gitpm-cli/src/main.rs`) plus `cli-catalog-commands` covers every generic stand command; `crates/gitpm/examples/api-parity.rs` exists (used by `uc_052`).
- No `.env` exists in this repository root or any parent directory (checked 2026-10-18: `/`, `/Users`, `/Users/dan`, `~/projects`, `~/projects/tilr`, `~/projects/tilr/git`). Dagger parses `.env` files above a module; this is why pm-dagger was a separate repository.
- pb engine: DaemonSet `dagger-engine` v0.21.10 in namespace `dagger`, kube context `promobot`; `dagger-pb` (`~/nexus/tools/dagger-pb`) exports `_EXPERIMENTAL_DAGGER_RUNNER_HOST=kube-pod://<pod>?context=promobot&namespace=dagger`; the CLI must be exactly 0.21.10; `kube-pod://` without `?context=` hangs.

## Goals / Non-Goals

**Goals:**
- One command, `./itests/dt`, runs the whole black-box suite against the `gitpm` built from the working tree.
- Every case maps to a spec requirement; uncovered requirements are visible in `openspec/coverage/`.
- Keep the old suite's strengths: one container per test, cached fixture snapshots, hermetic (loopback HTTP only, no real network).

**Non-Goals:**
- Replacing `cargo test --workspace` as the CI gate; Dagger is an additional, slower, broader suite.
- Testing the stand, the FFI bindings in other languages, or Tilr hosting.

## Decisions

### D1. Layout
```
itests/
  dagger.json           name gitpm-itests, engineVersion v0.21.10, sdk python
  pyproject.toml, uv.lock
  dt                    wrapper (POSIX sh), pattern of tilrd-itests/itests/dt
  src/gitpm_itests/main.py   the Dagger module (from pm_tests/main.py)
  gitpmtest/            helper (from pmtest), __main__: list | fixture | case
  fixtures/  scenarios/ (uc_NNN.py)
  tools/report.py  tools/history.py  tools/coverage.py
  README.md  .gitignore (/sdk /.venv /**/__pycache__ /results/ /.env)
```
The module takes `source: Directory` with `DefaultPath("/")` (the repository root, the Git context of the module) and an `Ignore` list keeping only `Cargo.toml`, `Cargo.lock`, `crates/` (minus `target/`), `tests/`, `.cargo/`, `README.md`, `openspec/specs/`; and `itests: Directory` with `DefaultPath("/itests")`. The wrapper refuses to run if a `.env` exists in `itests/` or any parent up to `/`, naming the file (cheap guard against the Dagger `.env` gotcha). Alternative: keep a separate repo — rejected by the owner's decision.

### D2. Wrapper
`./itests/dt [test] [--only rust,lint,scenarios] [--filter TEXT] [--jobs N] [--cached] [--open]`, `./itests/dt fixtures`, `fixture NAME`, `call FUNCTION …`, `history`, `clear [--keep N]`, `coverage`. `ENGINE=${DT_ENGINE:-dagger-pb}`; `DAGGER_NO_NAG=1`; the wrapper checks `dagger version` is v0.21.10 and fails otherwise. `--commit $(git rev-parse --short HEAD)` is passed for report labels. Results go to `itests/results/<stamp>/` (gitignored; old `pm-dagger/results/` is not ported). With a 1Password SSH agent run `env -u SSH_AUTH_SOCK ./itests/dt` (documented in README; the wrapper does not unset it silently).

### D3. Containers and Rust tests
Build container: `rust:1.94.0-bookworm` + clippy; the whole `CARGO_HOME` is one cache volume (`gitpm-itests-cargo-home`, so cargo's package-cache lock is shared by concurrent containers), `CARGO_TARGET_DIR=/target` on a cache volume. `cargo test --no-run --locked --workspace --message-format=json` lists every test executable; they and `gitpm` and the `api-parity` example are copied out of the cache into a layer. Runtime containers use identical paths: sources at `/src/gitpm`, the binary at `/target/debug/gitpm` (the `CARGO_BIN_EXE_gitpm` baked into CLI tests) and also on `PATH`, `gitpm-api` from `examples/api-parity`, workdir = the test's manifest dir. Git config: `GIT_CONFIG_GLOBAL=/src/gitpm/tests/gitconfig`, `GIT_CONFIG_NOSYSTEM=1` for `rust` (same as `.cargo/config.toml`), the pmtest gitconfig for scenarios; `GIT_TERMINAL_PROMPT=0`, `LANG=C.UTF-8`. Every libtest runs in its own container (`--exact NAME --test-threads=1`), so `integration-tests-from-bench`'s journey runs here too once it exists.

### D4. Scenario environment
Scenario containers set `GITPM_ROOT=/fx/ws`, `GITPM_CATALOG=/fx/catalog`, `GITPM_SOURCES=/fx/sources.txt`, `HOME=/root`, workdir `/fx`, and nothing else from the old set (`PM_CONFIG`, `PM_NAMESPACE`, `PM_PROJECT_DIR`, `PM_PUBLIC_URL`, `PM_REMOTE_CATALOG` are gone). `T.pm*` become `T.gitpm`, `T.ok`/`T.err` keep their contracts (`Error kind: KIND` match). A registry check refuses a case or fixture whose source mentions `PM_` env names or the `pm` binary.

### D5. Command mapping (stand `pm` → `gitpm`)

| Stand | gitpm (after `cli-catalog-commands`) | Note |
|---|---|---|
| `PM_ROOT`, `PM_CATALOG`, `PM_SOURCES` | `GITPM_ROOT`, `GITPM_CATALOG`, `GITPM_SOURCES` | defaults `./gitpm-workspace`, no catalog, `./sources.txt` |
| `PM_CONFIG`/`.pm.json`, `PM_NAMESPACE`, `namespace [NAME]` | — | obsolete: no default namespace (spec `package-ids`) |
| `init PATH`, `create PATH` (prompt, `--namespace`) | `init NS/NAME --into DIR` (`create` = alias) | no prompt |
| `fork ID PATH --namespace NS` | `fork ID --ns NS --into DIR [--replace]` | |
| `info` (two addresses, `pkg.tilr.dev` fallback) | `catalog show` | needs a catalog |
| `catalog` | `catalog links` | |
| `catalog-update [--sources F] [--publish]` | `catalog update [--sources F]` | `--publish` obsolete |
| `migrate-catalog` | `catalog migrate` | |
| `register SRC --ref R [--build-url --build-sha256]` | `catalog register SRC --ref R [--package ID] […]` | |
| `sync [--dry-run]` | — | obsolete (Tilr infrastructure) |
| `install`, `install-catalog`, `list`, `check`, `update`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `commit`, `remote`, `auth check` | same names | output per spec `cli` (`ID verb …`, no `OK:`), install idempotent, `dist/` holds only active packages |
| `pm-api` | `gitpm-api` | same example binary |

### D6. Per-UC classification
Static scan of `pm-dagger/itests/scenarios` (2026-10-18), per file: **as-is** = only the harness swap (D4) and mechanical renames (D5); **rewrite** = command syntax, asserted output (Russian stand text, `→ … for:` replacement line, `Created local commit`), `PM_*` assertions, or 0.4–0.5 behavior drift (dist active-only after disable/forget/replace, idempotent install instead of `already_installed`); **obsolete** = the whole file tests removed or excluded features. Totals: 15 as-is (103 cases), 47 rewrite (345 cases), 5 obsolete (58 cases). The scan over-approximates "dist active-only"; the first green run decides. Fixtures: `hello_installed`, `catalog`, `g1`, `g2`, `g4`, `g6_core*`, `deps*` port as-is; `g3_vendor_p_*`, `g5` (journey: `namespace` + `init PATH`, fork), `g6_base_installed` fork need rewriting (`--ns/--into`, `init NS/NAME --into`).

The full table is at the end of this document.

### D7. New cases (0.4–0.5 and change `cli-catalog-commands`)

| File | Covers (spec) |
|---|---|
| `uc_069` dist active-only | `dist`: disable/forget/replace remove `dist/ID`, enable/restore/`replace --back` redeploy, unmanaged paths warned |
| `uc_070` sync-dist | `dist`: 0.4-style workspace with inactive dists, `sync-dist --dry-run` lists, `sync-dist` fixes, second run `dist/ already holds exactly the active plugins.` |
| `uc_071` update all | `updates`: one line per package, `--dry-run` changes nothing (tree hash), kept back by constraints, new dependency installed |
| `uc_072` multi-package repositories | `multi-package-repos`: `init --multi`, one shared clone `repos/<name>-<hash>` for several packages, `install SRC ID`, `commit --package`, `catalog register --package`, `catalog update` publishes every package, fork of one package |
| `uc_073` metadata | `plugin-metadata`: `list --provides`, `title/description/tags`, `null`/`[]` shape, `catalog show` metadata lines |
| `uc_074` idempotent install | `install-uninstall`: `ID already installed (V)`, `; run update` hint, exit 0, workspace byte-identical |
| `uc_075` unified output | `cli`: verbs of every mutating command, no `OK:` lines, `--version`, per-command help |
| `uc_076` catalog group | `cli` (from `cli-catalog-commands`): five subcommands, deprecated aliases (stdout equal, stderr warning), unknown subcommand |
| `uc_077` partial publication (consumer side of `uc_051`) | `catalog`: loopback HTTP catalog with a stale plugin index or a missing `release.json`: install/check fail with `catalog`/`http` and recover after the file appears |

End-to-end journey: change `integration-tests-from-bench` stays a separate change (recommended). It is a hermetic `cargo test` (`crates/gitpm-cli/tests/bench_journey.rs`) that runs in the required CI gate in ~seconds; the Dagger suite runs it automatically as `rust:gitpm-cli/bench_journey::…` (D3). A second Dagger journey would duplicate it without new coverage. Alternative: merge it into this change as a Dagger scenario — rejected: it would move a CI-gated test into a pb-only suite.

### D8. Spec tags and coverage report
`@case(fixture=…, spec=["dist/dist-holds-exactly-the-active-packages", "cli/one-output-line-per-affected-package#update-all-lines"])`. A tag is `CAPABILITY/REQUIREMENT-SLUG[#SCENARIO-SLUG]`, slugs = lowercase heading text with non-alphanumerics collapsed to `-`, taken from `openspec/specs/<capability>/spec.md`. At least one tag per case is required; `spec=["none:REASON"]` is allowed for harness self-checks. `./itests/dt coverage` runs on the host (`uv run --no-project python itests/tools/coverage.py`, stdlib only, no Dagger): it imports the registry, parses the specs, and writes `openspec/coverage/coverage.md` (per capability: requirement, scenario, Dagger cases, cargo tests named in "Verified by"/"covered by" text, last-run status from `itests/results/latest/report.json` when present) and `openspec/coverage/coverage.json`. It exits 1 on an untagged case or a tag naming no existing requirement/scenario. The registry (`python3 -m gitpmtest list`) performs the same tag validation inside Dagger with `openspec/specs/` mounted, so a bad tag fails the run early. Expected failures: `@case(xfail="catalog-streaming-size-limit")` reports `XFAIL` (not a failure) and `XPASS` as a failure, so a fix forces the marker's removal. Tags resolve against `openspec/specs/` only, so requirements added by `cli-catalog-commands` become taggable once that change is archived (its deltas synced).

### D9. Dagger gotchas carried over
- The `test` function is `@function(cache="never")`; fixtures stay cached layers keyed by their inputs (source tier: module + helper; gitpm tier: also the binary).
- Reports are written as one bundle JSON unpacked by a single exec, never ~900 `with_new_file` calls (export breaks on the overlayfs lower-dir limit).
- overlayfs: no rename/hardlink across lower layers — cases copy `/fx` before mutating where they rename (kept from pmtest).
- The whole `CARGO_HOME` is mounted (D3), not only the registry.
- Identical container layout for `CARGO_BIN_EXE_*` and `CARGO_MANIFEST_DIR` (D3).
- CLI version pinned to 0.21.10 (D2); `SSH_AUTH_SOCK` note (D2).

### D10. CI
No GitHub Actions job now: the suite runs on pb (`dagger-pb` from the Mac, or the in-cluster Job pattern of `charts/dagger-engine/test/run-test.sh`). A later optional, non-required job (manual `workflow_dispatch`, local engine via `dagger/dagger-for-github` pinned to 0.21.10) can run `./itests/dt --only scenarios`; it would not be added to the required checks of `release-process`.

### D11. Transactions and crash recovery
Not applicable to gitpm: the suite only writes inside containers and to `itests/results/` and `openspec/coverage/` on the host. Each run writes a new `results/<stamp>` and flips `results/latest` last, so an interrupted run leaves the previous `latest` intact.

## Implementation notes (2026-10-18, where reality differs)

- **Command names (D5):** `cli-catalog-commands` is not implemented (awaiting the owner's decision), so scenarios use the current names `info`, `catalog`, `catalog-update`, `register`, `migrate-catalog`. `gitpmtest.COMMANDS` maps each name to the argv `T.gitpm` runs; the rename is a change of that mapping (plus the help/usage-text assertions in `uc_001`/`uc_063`). `uc_076` (catalog group and deprecated aliases) is not written yet for the same reason (task 5.5).
- **Build sources (D1):** the build containers get `source` without `openspec/`, so spec edits (tags) do not invalidate the Cargo layer; only the registry mounts `openspec/specs/` (at `/specs`, `GITPMT_SPECS`). The `itests` directory is mounted with `sdk/`, `.venv/`, `results/`, `src/` and `__pycache__` ignored.
- **Memory (D3/D9):** the first cold run OOM-killed the pb engine (24 GiB limit) with the test build and clippy compiling in parallel; builds now use `CARGO_BUILD_JOBS=6` and clippy waits for the test build when both run.
- **Filters (D2):** `--filter` also accepts a comma list of substrings (any matches), e.g. `--filter uc_020:,uc_048:`. `--commit` is `git rev-parse --short HEAD`, suffixed `+dirty` for a dirty tree.
- **Statuses (D8):** `BLOCKED` does not fail the run (only `FAIL` and `XPASS` do; a failed fixture is itself `FAIL`). A module-level `SPEC = [...]` is a fallback for loop-generated cases; every case of the port has its own tag except `uc_061`'s generated bad-ID cases. `xfail="bug: …"` is the convention for a defect without a change (none needed so far).
- **Counts (D6):** 62 ported files + 8 new (`uc_069`–`uc_075`, `uc_077`) = 70 files, 638 scenario cases (loop-generated cases included), 42 fixtures; 314 Rust tests (= `cargo test --workspace -- --list`). Dropped cases are listed in `itests/README.md` "Not ported". Most "rewrite" files needed only the dist-active-only/idempotent-install/English-output changes; the first mechanical port already passed 417/485.
- **Legacy check:** the registry rejects `PM_*` names, `"pm"`/`"pm-api"` literals and `.pm(` calls anywhere in a scenario/fixture module; `uc_002:legacy_root_variable_is_ignored` spells the old variable as `"PM" "_ROOT"` on purpose.

## Risks / Trade-offs

- [345 cases need rewriting] → port in UC order by capability (tasks), a file at a time, each green before the next; obsolete cases are deleted, not commented out.
- [Behavior drift hides real regressions behind "expected" rewrites] → each rewrite that changes an expectation cites the spec requirement and the 0.x "Changed in" README entry in the tag.
- [pb availability] → `DT_ENGINE=dagger` local engine works for the same module.
- [Python in a Rust repository] → `itests/` is self-contained (uv, own pyproject), invisible to cargo, cargo-deny and the release; gitleaks still scans it.

## Migration Plan

1. `cli-catalog-commands` lands on `dev`.
2. This change ports the module and cases (tasks), runs green on pb, and generates `openspec/coverage/`.
3. Cutover and removal of `pm-dagger` and `temp-plugin-manager`: task TILR-13, after the owner's confirmation; until then the old suite is left as is (it no longer builds against `dev`).

Rollback: delete `itests/` and `openspec/coverage/`; nothing else depends on them.

## Per-UC table (D6)

| UC | Title | Cases | Category | Why / what changes | Old catalog names (mechanical rename) |
|---|---|---|---|---|---|
| uc_001 | CLI and isolated environment | 3 | rewrite | namespace/--namespace; init/create NS/NAME --into; fork --ns --into; sync/remote-catalog cases dropped; PM_* env asserted; help command list | info, catalog-update, migrate-catalog, register |
| uc_002 | Environment, wrapper and path precedence | 7 | rewrite | stand Russian output; namespace/--namespace; init/create NS/NAME --into; sync/remote-catalog cases dropped; PM_* env asserted; stand output/defaults; wrapper/.env cases dropped; precedence → `GITPM_*` | info, catalog-update, register |
| uc_003 | Namespace configuration and validation | 6 | rewrite | stand Russian output; namespace/--namespace; init/create NS/NAME --into; PM_* env asserted; precedence cases dropped; namespace validation kept via `init NS/NAME` |  |
| uc_004 | Init and create scaffold | 5 | rewrite | namespace/--namespace; init/create NS/NAME --into |  |
| uc_005 | Interactive init and EOF | 7 | obsolete | interactive init prompt; gitpm init takes NS/NAME --into DIR, never prompts (package-ids: No default namespace) |  |
| uc_006 | Author release commit | 3 | rewrite | init/create NS/NAME --into |  |
| uc_007 | Remote configuration | 3 | rewrite | init/create NS/NAME --into |  |
| uc_008 | Auth check local and empty repositories | 5 | rewrite | init/create NS/NAME --into |  |
| uc_009 | Auth URL validation and failure diagnostics | 9 | as-is |  |  |
| uc_010 | Direct install by tag and commit | 4 | rewrite | idempotent install |  |
| uc_011 | Stable release selection | 4 | as-is |  |  |
| uc_012 | Dev channel and explicit references | 7 | as-is |  |  |
| uc_013 | Independent namespaces and workspaces | 5 | rewrite | dist active-only |  |
| uc_014 | Invalid manifests and missing payload | 13 | rewrite | init/create NS/NAME --into; idempotent install | register |
| uc_015 | Stable update and no-op | 5 | as-is |  |  |
| uc_016 | Explicit update and version identity | 8 | as-is |  |  |
| uc_017 | Rollback and one-version history | 7 | rewrite | dist active-only |  |
| uc_018 | Local edit and unpublished commit protection | 1 | rewrite | dist active-only |  |
| uc_019 | Failed update transaction | 5 | as-is |  |  |
| uc_020 | Enable and disable | 6 | rewrite | dist active-only |  |
| uc_021 | Forget and restore | 8 | rewrite | dist active-only; idempotent install |  |
| uc_022 | Uninstall, exact ID and namespace guards | 6 | rewrite | namespace/--namespace; fork --ns --into; BLOCKED unless case-insensitive FS (exit 77, kept) | register |
| uc_023 | Fork preserves history, edits and provenance | 6 | rewrite | namespace/--namespace; fork --ns --into |  |
| uc_024 | Fork release independence and branch collision | 9 | rewrite | namespace/--namespace; fork --ns --into |  |
| uc_025 | Fork invalid destinations | 12 | rewrite | namespace/--namespace; fork --ns --into |  |
| uc_026 | Legacy compatibility and path collisions | 7 | rewrite | namespace/--namespace; init/create NS/NAME --into | info, register |
| uc_027 | Register and immutable catalog releases | 5 | as-is |  | register |
| uc_028 | Catalog v2 tree and hashes | 3 | as-is |  | register |
| uc_029 | Catalog install exact/latest and integrity | 9 | as-is |  |  |
| uc_030 | Build metadata validation | 9 | as-is |  | register |
| uc_031 | Catalog generation from sources | 3 | rewrite | PM_* env asserted | catalog-update |
| uc_032 | Catalog generation validation and retry | 7 | as-is |  | catalog-update, register |
| uc_033 | Catalog tag identity and version rules | 6 | rewrite | stand Russian output; namespace/--namespace; fork --ns --into | info, catalog-update |
| uc_034 | Info and catalog read-only browsing | 9 | rewrite | stand Russian output; sync/remote-catalog cases dropped; PM_* env asserted; `info` → `catalog show`; remote-catalog fallback cases dropped | info |
| uc_035 | Check statuses and selective HTTP reads | 11 | as-is |  |  |
| uc_036 | HTTP catalog errors and unsafe references | 11 | as-is |  | info, register |
| uc_037 | V1 migration and backward compatibility | 6 | rewrite | stand Russian output; sync/remote-catalog cases dropped | info, migrate-catalog, register |
| uc_038 | Sync dry-run and upload validation | 15 | obsolete | sync to R2 (Tilr infrastructure, excluded) | migrate-catalog, register |
| uc_039 | Sync upload failures and cache purge | 19 | obsolete | sync upload/purge (excluded) | catalog-update |
| uc_040 | Workspace/catalog locking and recovery | 8 | rewrite | dist active-only | info, catalog-update, migrate-catalog, register |
| uc_041 | Symlinks, special files and managed-path safety | 10 | rewrite | namespace/--namespace; init/create NS/NAME --into; fork --ns --into; sync/remote-catalog cases dropped; dist active-only | register |
| uc_042 | Author journey (create → release → publish to a local catalog) | 4 | rewrite | namespace/--namespace; init/create NS/NAME --into; fork --ns --into; sync/remote-catalog cases dropped; stand output/defaults | catalog-update |
| uc_043 | Ignored files, unpublished refs and uninstall confirmation | 2 | rewrite | namespace/--namespace; fork --ns --into |  |
| uc_044 | Fork local HEAD, staged changes and fork of fork | 5 | rewrite | namespace/--namespace; fork --ns --into | register |
| uc_045 | Corrupt state and missing managed files | 6 | rewrite | dist active-only | register |
| uc_046 | Write failure and interrupted transactions | 8 | rewrite | dist active-only | catalog-update, register |
| uc_047 | Offline local lifecycle | 6 | rewrite | PM_* env asserted; dist active-only |  |
| uc_048 | State transition matrix | 6 | rewrite | namespace/--namespace; fork --ns --into; dist active-only; idempotent install |  |
| uc_049 | Private Git success and revoked access | 3 | as-is | optional private-Git infrastructure | register |
| uc_050 | Real concurrent CLI mutations | 5 | rewrite | dist active-only; idempotent install | info, register |
| uc_051 | Publish and consumer consistency under failure | 7 | obsolete | publish via sync + stubbed bucket (excluded); consumer-side cases re-expressed as new HTTP catalog cases | info, catalog-update |
| uc_052 | Library API and CLI behavioral parity | 10 | rewrite | namespace/--namespace; fork --ns --into; stand output/defaults; dist active-only; idempotent install; `pm-api` → `gitpm-api` (`crates/gitpm/examples/api-parity.rs`) |  |
| uc_053 | Required fork replacement of a dependency | 4 | rewrite | stand Russian output; stand output/defaults; dist active-only |  |
| uc_054 | Required replacement compatibility and conflicts | 11 | rewrite | namespace/--namespace; fork --ns --into |  |
| uc_055 | Required replacement update rollback and failure | 7 | as-is |  |  |
| uc_056 | Required switch back and dependent deletion guards | 12 | rewrite | stand output/defaults; dist active-only |  |
| uc_057 | Single selected catalog policy | 7 | rewrite | stand Russian output; sync/remote-catalog cases dropped; PM_* env asserted; `info` two-address/public fallback and `sync` cases dropped | info |
| uc_058 | Required plugin dependencies without replacement | 15 | rewrite | PM_* env asserted | register |
| uc_059 | Required dependent guards on disable, forget and uninstall | 8 | rewrite | stand Russian output |  |
| uc_060 | Fork status and upstream version visibility | 4 | rewrite | stand Russian output; namespace/--namespace; fork --ns --into | register |
| uc_061 | Identifier, tag and version edge cases | 14 | rewrite | stand Russian output; namespace/--namespace; init/create NS/NAME --into; fork --ns --into; dist active-only | info, catalog-update, register |
| uc_062 | Repeated mutations (idempotency) | 13 | rewrite | namespace/--namespace; idempotent install | catalog-update, migrate-catalog, register |
| uc_063 | CLI usage errors, help and output modes | 11 | rewrite | stand Russian output; namespace/--namespace; init/create NS/NAME --into; fork --ns --into; sync/remote-catalog cases dropped | info, catalog-update, migrate-catalog, register |
| uc_064 | Sync against existing remote state | 10 | obsolete | sync against remote bucket state (excluded) | catalog-update, migrate-catalog, register |
| uc_065 | Review regressions: refs, symlinks, transport and managed names | 20 | rewrite | stand Russian output; namespace/--namespace; init/create NS/NAME --into; fork --ns --into; dist active-only; `oversized_streamed_catalog_response` xfail until change `catalog-streaming-size-limit` | info, register |
| uc_066 | Fork replacement main scenario, happy path | 9 | rewrite | stand Russian output; namespace/--namespace; fork --ns --into; dist active-only | register |
| uc_068 | Consumer journey: install from catalog → lifecycle → uninstall | 7 | rewrite | dist active-only | register |
