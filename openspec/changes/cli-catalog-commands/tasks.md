# Tasks

## 1. Catalog group dispatch

- [ ] 1.1 In `crates/gitpm-cli/src/main.rs` resolve `catalog SUB` (`show|links|update|migrate|register`) to an effective two-word command before option validation and the argument-count check; route each to the existing code path of `info`, `catalog`, `catalog-update`, `migrate-catalog`, `register`; verify with a new test `catalog_group_runs_every_subcommand` in `crates/gitpm-cli/tests/lifecycle.rs` (update with two tags prints `Catalog ready: 2 release(s) checked from sources.`, register prints JSON, migrate prints `Catalog migrated: …`, show lists the plugin, links prints JSON)
- [ ] 1.2 Validation messages and the missing-catalog message use the two-word name (`--sources requires catalog update`, `catalog show needs a catalog: …`); `--ref`, `--package`, `--build-url/--build-sha256` accepted for `catalog register`; verify by extending `usage_errors_name_the_problem_and_create_nothing` and `catalog_commands_name_the_missing_catalog_and_create_nothing` with the new spellings (nothing created in the temp dir)
- [ ] 1.3 Unknown word after `catalog` fails with `unknown catalog command: X; use gitpm catalog --help` before opening anything; verify with a test asserting the message, exit 1 and an empty temp dir
- [ ] 1.4 Change the overview limit line in `crates/gitpm-cli/src/lib.rs` to `… gitpm --catalog <branch location> catalog show`; verify with the existing overview-limit test updated to the new text

## 2. Deprecated aliases

- [ ] 2.1 `info`, bare `catalog`, `catalog-update`, `migrate-catalog`, `register` print ``Warning: `gitpm OLD` is deprecated; use `gitpm catalog NEW` `` on stderr and otherwise behave identically; verify with `deprecated_catalog_names_match_the_group_output` (stdout byte-equal to the new spelling, warning present, same exit status) and keep every existing CLI test that uses the old names green
- [ ] 2.2 Help: a `Catalog` section with the five subcommand blocks; `gitpm catalog --help` prints all of them, `gitpm catalog SUB --help` one; old names keep a block that names their replacement; verify `tests::every_command_has_a_usage_block` and `command_help_prints_only_that_command` extended for `catalog register --help`

## 3. Documentation

- [ ] 3.1 README: command reference uses the new names, a "Changed in 0.6" entry lists the five renames and the deprecation, and the Settings block names `catalog update` for `--sources`; verify `grep -n "catalog-update\|migrate-catalog" README.md` hits only the "Changed in" and deprecation lines
- [ ] 3.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; all green

## Workflow follow-up

- A later change removes the deprecated names (**BREAKING**), after `dagger-integration-tests` and the README use only the group.
- Archive: record the implementing commit/PR in proposal.md.
