# Tasks

## 1. Library

- [ ] 1.1 `open_with` validates Options before creating root, folders, state or lock; a missing local catalog location fails with `invalid_argument`; verify `workspace::open_with_validates_options_before_creating_anything` and `workspace::a_missing_catalog_location_is_refused_before_creating_anything` (temp dir unchanged)
- [ ] 1.2 `list_with(root, ListOptions)` read-only list view with `provides` filter; verify `list::list_with_never_creates_or_locks_a_workspace` and the filter test moved from the CLI
- [ ] 1.3 Every mutating result carries `dist: DistSync`; verify `workspace::mutating_results_carry_the_dist_report`
- [ ] 1.4 `Error::NotDeployed` (kind `not_deployed`) from `sync_dist` after committing; README kinds list; verify `error::every_variant_has_its_stable_kind`, `error::readme_lists_exactly_the_error_kinds` and a sync-dist test with a missing clone
- [ ] 1.5 `ReplacementPlan.installed_version`/`fork_enabled`; pending plan in fork results; `fork_replace_resume`; verify `replacement_plan_carries_installed_version_and_fork_enabled` and `fork_replace_pending_replacement_is_resumed_after_confirmation`
- [ ] 1.6 `UpdateAllPlan.unchanged`; `commit_package` returns `Commit`; verify `update_all_plan_lists_unchanged_packages` and `scaffold::commit_package_returns_package_version_and_tag`
- [ ] 1.7 Neutral messages: remove command names and R2 text from library messages (scaffold, replace, catalog, forgotten-ID refusals); `init NAME` → `invalid_id`; verify `error::messages_name_no_cli_command` (grep of library message strings for `gitpm `, `--`, `R2`)
- [ ] 1.8 `update`/`update all` keep the skipped broken tags of release discovery and return them in `RevisionChange.skipped` (TILR-103); verify `releases::bad_historical_tag_does_not_hide_valid_releases` extended with `update` and `update all` asserting the skipped tag

## 2. CLI

- [ ] 2.1 Remove the catalog fallback, the CLI-side `--channel`/`--ref` combination check and the CLI `list` builder; print only library results; verify `usage_errors_name_the_problem_and_create_nothing` and `list_reports_metadata_present_or_absent_and_filters_by_provides`
- [ ] 2.2 `Hint:` lines per error kind; prompts on stderr; fork --replace uses the pending plan and `fork_replace_resume`; replace/update-all lines from plan data only; verify CLI tests for hints, `uninstall_on_tty_asks_before_losing_work` (stderr) and `fork_replace_keeps_the_fork_and_says_how_to_finish_when_a_step_fails`
- [ ] 2.3 Print `Warning: skipped release tag NAME: reason` from `RevisionChange.skipped` for `update` and `update all`; verify a CLI test updating past a broken historical tag

## 3. FFI

- [ ] 3.1 Free functions `list`, `check`, `readState` (full state view, replaces `readInstalled`, which is removed), `catalogLinks`; drop `PackageManager.check`; `RevisionChange.skipped`; add `forkReplaceResume`, `PmError.NotDeployed` and the new record fields; verify `records_convert_field_by_field`, `ffi_covers_every_workspace_and_catalog_read_command` and Swift binding generation via `scripts/build-apple.sh`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: cases for `--catalog ./missing`, `list` during a held lock (`GITPM_TEST_HOLD_LOCK`), hints and `not_deployed`; verify `./itests/dt --only scenarios`
- [ ] 4.2 README "Changed in 0.6" (BREAKING: missing catalog path, `not_deployed`, FFI changes), CLI output and FFI sections; verify by reading the rendered sections against the specs
- [ ] 4.3 `openspec/config.yaml`: check that the thin-CLI rule (TILR-33) holds after this change and state it in `rules.specs` ("Domain rules and option combinations live in the library; CLI specs cover argv/env parsing, printing, TTY prompts and hints per error kind; every CLI capability has an FFI equivalent"); keep `rules.design` "Thin CLI"; verify by reading the file against the code
- [ ] 4.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
