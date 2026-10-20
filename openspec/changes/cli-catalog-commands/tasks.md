# Tasks

## 1. Catalog group dispatch

- [x] 1.1 In `crates/gitpm-cli/src/main.rs` resolve `catalog SUB` (`show|links|update|register`) to an effective two-word command before option validation and the argument-count check; route each to the existing code path of `info`, `catalog`, `catalog-update`, `register`; verify with a new test `catalog_group_runs_every_subcommand` in `crates/gitpm-cli/tests/lifecycle.rs` (update with two tags prints `Catalog ready: 2 release(s) checked from sources.`, register prints JSON, show lists the package, links prints JSON)
- [x] 1.2 Validation messages and the missing-catalog message use the two-word name (`--sources requires catalog update`, `catalog show needs a catalog: …`); `--ref`, `--package`, `--build-url/--build-sha256` accepted for `catalog register`; verify by extending `usage_errors_name_the_problem_and_create_nothing` and `catalog_commands_name_the_missing_catalog_and_create_nothing` with the new spellings (nothing created in the temp dir)
- [x] 1.3 Bare `catalog` fails with `catalog needs a subcommand: show, links, update or register; use gitpm catalog --help`, an unknown word with `unknown catalog command: X; use gitpm catalog --help`, both before opening anything; verify with a test asserting the messages, exit status 1 and an empty temp dir
- [x] 1.4 Remove `info`, `catalog-update` and `register` from the command table (BREAKING); verify `gitpm info` fails with `unknown command: info; use --help` in `help_names_the_binary_and_removed_commands_are_unknown`
- [x] 1.5 Change the overview limit line in `crates/gitpm-cli/src/lib.rs` to `… gitpm --catalog <branch location> catalog show`; verify with the existing overview-limit test updated to the new text
- [x] 1.6 The CLI hint for the (already neutral) `invalid_argument` of writing to an HTTP(S) catalog names `gitpm catalog update`; verify with a CLI test asserting the library message and the hint line

## 2. Help

- [x] 2.1 A `Catalog` section with the four subcommand blocks; `gitpm catalog --help` prints all of them, `gitpm catalog SUB --help` one; verify `tests::every_command_has_a_usage_block` and `command_help_prints_only_that_command` extended for `catalog register --help`

## 3. Documentation, specs and Dagger suite

- [x] 3.1 README: command reference uses the new names and a "Changed in 0.6" entry lists the four renames as BREAKING; the Settings block names `catalog update` for `--sources`; verify `grep -n "catalog-update\|\binfo\b" README.md` hits only the "Changed in" lines
- [x] 3.2 Before archive, add MODIFIED deltas for every remaining spec requirement that names `info`, bare `catalog`, `catalog-update` or `register` as a command (`grep -rn` over `openspec/specs/` after the chain is archived, including requirements the chain added such as catalog "Verified catalogs" and "Published versions are immutable"), so archiving leaves no old name; verify `openspec validate cli-catalog-commands --strict`
- [x] 3.3 Dagger: remap `gitpmtest.COMMANDS` to the new spellings, update the help/usage assertions in `uc_001`/`uc_063`, and write `uc_076` (four subcommands, bare `catalog`, unknown subcommand, old names unknown; no alias case) tagged into `cli`; verify `./itests/dt --only scenarios --filter uc_0` is green and `./itests/dt coverage` resolves every tag (moved from `2026-10-18-dagger-integration-tests` tasks 0.1 and 5.5)
- [ ] 3.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; all green

## Workflow follow-up

- Archive: record the implementing commit/PR in proposal.md.
