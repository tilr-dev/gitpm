# Tasks

## 0. Dependency

- [x] 0.1 Moved to `cli-catalog-commands` (TILR-8): the suite does not depend on it. (Original text, not met: `cli-catalog-commands` awaits the owner's decision; the suite uses the current command names through `gitpmtest.COMMANDS`, see design "Implementation notes") Confirm `cli-catalog-commands` is implemented on `dev` and archived (its deltas synced into `openspec/specs/`); verify `gitpm catalog update --help` prints the block and `openspec/specs/cli/spec.md` contains "Catalog command group"

## 1. Module and Rust kinds

- [x] 1.1 Create `itests/` (dagger.json `gitpm-itests` engine v0.21.10, pyproject/uv.lock, `.gitignore` with `/sdk /.venv /**/__pycache__ /results/ /.env`, `src/gitpm_itests/main.py`) with `source` = `DefaultPath("/")` plus the Ignore list of design D1; verify `dagger-pb -m itests functions` lists `test`, `fixtures`, `fixture`, `build`
- [x] 1.2 Port the build, `rust` and `lint` kinds for the gitpm workspace only (identical layout, `CARGO_HOME` cache volume, `GIT_CONFIG_GLOBAL=/src/gitpm/tests/gitconfig`, `test` with `cache="never"`, bundled report); verify `./itests/dt --only rust,lint` passes and the rust count equals `cargo test --workspace -- --list | grep -c ': test$'`
- [x] 1.3 Write `itests/dt` (design D2: default `dagger-pb`, `DT_ENGINE=dagger`, version check v0.21.10, `.env` guard, `--open`, `fixtures`, `fixture`, `call`, `history`, `clear`); port `tools/report.py` and `tools/history.py`; verify with a `.env` touched in the repo root (refused, file named) and a successful `./itests/dt --only lint`
- [x] 1.4 `itests/README.md`: how to run on pb and locally, filters, `env -u SSH_AUTH_SOCK`, results layout; verify the README commands run as written

## 2. Harness swap

- [x] 2.1 Port `pmtest` as `gitpmtest`: `T.gitpm/gitpm_ok/gitpm_err`, scenario env `GITPM_ROOT/GITPM_CATALOG/GITPM_SOURCES` only (design D4), `gitpm-api` from `examples/api-parity`; registry refuses `PM_*` and the `pm` binary; verify a deliberately bad scratch case is refused by `python3 -m gitpmtest list`
- [x] 2.2 Add `spec=[…]` tags to `@case` with validation against `openspec/specs/` (design D8) and `xfail=CHANGE` with XFAIL/XPASS reporting; verify an unknown tag fails the registry and an xfail case that passes turns the run red
- [x] 2.3 Port the source-tier and gitpm-tier fixtures (rewrite `g3_vendor_p_*`, `g5`, `g6_base_installed` to `init NS/NAME --into`, `fork --ns --into`, `catalog register|update`); verify `./itests/dt fixtures` lists 41 fixtures (minus any whose only users are obsolete) and each fixture's check passes
- [x] 2.4 Port the 15 as-is files (`uc_009 011 012 015 016 019 027 028 029 030 032 035 036 049 055`) with mechanical renames and spec tags; verify `./itests/dt --only scenarios --filter` per file is green

## 3. Rewrites, by capability

- [x] 3.1 CLI and environment: `uc_001 002 003 004 006 007 008 063` (help command list incl. `sync-dist` and the catalog group, `GITPM_*` precedence, `init NS/NAME --into`, English output); verify green on pb with tags into `cli`, `authoring`, `package-ids`, `git-safety`
- [x] 3.2 Install/update/lifecycle: `uc_010 013 014 017 018 020 021 040 045 046 047 048 050 058 059 062 068` (idempotent install, dist active-only expectations); verify green with tags into `install-uninstall`, `updates`, `dist`, `transactions-locking`, `dependencies`
- [x] 3.3 Fork and replacement: `uc_022 023 024 025 043 044 052 053 054 056 060 066` (`fork --ns --into`, replacement output `ORIGINAL replaced by FORK V for: …`, `replace --back` message); verify green (uc_022 may be BLOCKED) with tags into `fork-replacement`, `dist`
- [x] 3.4 Catalog: `uc_026 031 033 034 037 041 042 057 061 065` (catalog group names, no remote-catalog fallback, sync cases removed); mark `uc_065:oversized_streamed_catalog_response` `xfail="catalog-streaming-size-limit"`; verify green/XFAIL with tags into `catalog`, `package-ids`, `git-safety`

## 4. Obsolete

- [x] 4.1 Do not port `uc_005 038 039 051 064` and the dropped cases inside rewritten files; record each dropped case and its reason in `itests/README.md` ("Not ported"); verify the list's count matches design D6 (58 cases in obsolete files plus the dropped cases named in 3.x)

## 5. New cases

- [x] 5.1 `uc_069` dist active-only and `uc_070` sync-dist (design D7); verify green with tags into `dist`
- [x] 5.2 `uc_071` update all (+`--dry-run` tree hash unchanged); verify green with tags into `updates`, `cli`
- [x] 5.3 `uc_072` multi-package repositories (one shared clone `repos/<name>-<hash>`); verify green with tags into `multi-package-repos`
- [x] 5.4 `uc_073` metadata and `uc_074` idempotent install; verify green with tags into `plugin-metadata`, `install-uninstall`
- [x] 5.5 `uc_075` unified output (done, green); `uc_076` catalog group moved to `cli-catalog-commands` (TILR-8, its tasks), without deprecated aliases
- [x] 5.6 `uc_077` partial publication over loopback HTTP (consumer side of old `uc_051`); verify green with tags into `catalog`

## 6. Coverage report

- [x] 6.1 `itests/tools/coverage.py` and `./itests/dt coverage` writing `openspec/coverage/coverage.md` and `coverage.json` (design D8); verify it runs without Dagger, lists every capability of `openspec/specs/`, marks an untagged requirement as uncovered, and exits 1 on an untagged case
- [x] 6.2 Generate and commit-ready `openspec/coverage/` from the green run; verify `coverage.json` reports last-run status for every case

## 7. Integration

- [x] 7.1 Full run `./itests/dt` on pb: zero FAIL, only the documented XFAIL/BLOCKED; verify `itests/results/latest/summary.txt`
- [x] 7.2 Same run with `DT_ENGINE=dagger` on a local engine; verify the same totals
- [x] 7.3 README "Tests" section names `./itests/dt` and `openspec/coverage/`; run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (unchanged, green)

## Workflow follow-up

- TILR-13: cutover and removal of `~/projects/tilr/pm-dagger` and `~/projects/tilr/temp-plugin-manager`, only after the owner confirms.
- Optional later: a non-required `workflow_dispatch` CI job (design D10).
- Archive: record the implementing commit/PR in proposal.md.
