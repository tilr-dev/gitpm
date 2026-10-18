# Tasks

## 1. Library

- [x] 1.1 One guard: dependents = consumers that break after the binding is dropped (ORIGINAL installed and satisfying → allowed); disable/forget/uninstall of an active fork drop the binding and redeploy ORIGINAL; `HasDependents.replacing` removed; verify `dependents_guard_disable_forget_and_uninstall`, `removing_an_active_fork_guards_only_consumers_the_original_breaks` and `disable_removes_and_enable_restores_also_around_a_replacement` (updated expectations)
- [x] 1.2 `replace --with` requires ORIGINAL installed; `switch_back` with ORIGINAL absent drops the binding under the guard; verify `replace_requires_the_original_and_back_drops_an_absent_one`
- [x] 1.3 Uninstall/forget of a recorded package without clone drop record and dist; verify `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`
- [x] 1.4 `uninstall(id, confirmed, force)` (delete `uninstall_with`); `HasDependents.lost_work`; `UninstallPlan::refusal`; verify `uninstall::confirmation_is_bound_to_the_planned_lost_work` and `error::display_is_the_complete_message`
- [x] 1.5 Reactivating ORIGINAL (binding dropped by disable/forget/uninstall of the fork, or switch back) checks ORIGINAL's direct dependencies with the enable helper (forceable for the removals); verify `dropping_a_binding_checks_the_original_s_dependencies`
- [x] 1.6 `forget` returns `ForgetResult { clone_kept, dist }`; verify `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record` and `forget_says_whether_the_clone_was_kept`
- [x] 1.7 State load refuses a binding to a disabled package (`invalid_data`); `ReplacementPlan.fork_enabled` removed; verify `a_binding_to_a_disabled_fork_is_invalid_state` and `replacement_plan_carries_installed_version`
- [x] 1.8 `local_changes` messages without "requires force"; verify `lost_work` message tests and `error::messages_name_no_cli_command`
- [x] 1.9 `switch_back(original, confirm, force)` guards consumers a missing or disabled original does not serve; `UninstallPlan.dependency_conflict` in `refusal()`; `SkippedDist.clone_missing`; verify `switch_back_to_a_disabled_original_is_guarded`, `dropping_a_binding_checks_the_original_s_dependencies`, `uninstall_refusal_of_a_reactivation_comes_before_the_prompt` and `skipped_dists_say_structurally_whether_the_clone_is_missing`

## 2. CLI

- [x] 2.1 Uninstall passes the plan's items as `confirmed` (after `y`, or with `--force`) and reports `UninstallPlan::refusal` instead of calling `uninstall` for it; prints lost-work lines for `has_dependents`; `replace --back` output for an absent original; new missing-clone warning text; verify `uninstall_on_tty_asks_before_losing_work`, `uninstall_without_tty_refuses_and_lists_lost_work_including_stash` and `a_missing_clone_is_reported_and_blocks_only_its_own_dist`

## 3. FFI

- [x] 3.1 `uninstall(confirmed, force)`, `PmError.HasDependents.lostWork` (no `replacing`), free function `uninstallRefusal(plan)`; Swift bindings build; verify `ids_and_lost_work_items_are_preserved`

## 4. Specs, tests, docs

- [x] 4.1 Dagger: disable of an active fork with and without breaking consumers, `replace --back` with the original absent, uninstall of a package whose clone was deleted; verify `./itests/dt --only scenarios`
- [x] 4.2 README removal levels, replacement rules and "Changed in 0.6"; verify by reading against the specs
- [x] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
