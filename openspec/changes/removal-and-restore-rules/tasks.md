# Tasks

## 1. Library

- [ ] 1.1 One guard: dependents = consumers that break after the binding is dropped (ORIGINAL installed and satisfying → allowed); disable/forget/uninstall of an active fork drop the binding and redeploy ORIGINAL; verify `dependents_guard_disable_forget_and_uninstall` and `disable_removes_and_enable_restores_also_around_a_replacement` (updated expectations)
- [ ] 1.2 `replace --with` requires ORIGINAL installed; `switch_back` with ORIGINAL absent drops the binding under the guard; verify `replace_requires_the_original_and_back_drops_an_absent_one`
- [ ] 1.3 Uninstall/forget of a recorded package without clone drop record and dist; verify `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`
- [ ] 1.4 `uninstall(id, confirmed, force)`; `HasDependents.lost_work`; verify `uninstall::confirmation_is_bound_to_the_planned_lost_work` and `error::display_is_the_complete_message`

## 2. CLI

- [ ] 2.1 Uninstall passes the plan's items as `confirmed` (after `y`, or with `--force`); prints lost-work lines for `has_dependents`; new missing-clone warning text; verify `uninstall_on_tty_asks_before_losing_work`, `uninstall_without_tty_refuses_and_lists_lost_work_including_stash` and `a_missing_clone_is_reported_and_blocks_only_its_own_dist`

## 3. FFI

- [ ] 3.1 `uninstall(confirmed, force)`, `PmError.HasDependents.lostWork`; verify `ids_and_lost_work_items_are_preserved`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: disable of an active fork with and without breaking consumers, `replace --back` with the original absent, uninstall of a package whose clone was deleted; verify `./itests/dt --only scenarios`
- [ ] 4.2 README removal levels, replacement rules and "Changed in 0.6"; verify by reading against the specs
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
