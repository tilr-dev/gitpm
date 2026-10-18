# Tasks

## 1. Library

- [x] 1.1 `update all` leaves broken consumers (`deps::broken_dependencies`) at their installed revision and out of the solve (`upgrade::solve` drops their unsatisfied dependencies); they are listed in `unchanged` with `UnchangedReason::BrokenDependency`; removed dependencies are never reinstalled; no `dependency_conflict` for a broken state; an inactive consumer takes a disabled provider as given; verify `update_all_skips_broken_consumers_and_never_reinstalls` (with and without a catalog), `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`, `disabled_packages_satisfy_no_dependency`
- [x] 1.2 `HeldBack.reason: HeldBackReason` (`Constraints`, `MissingDependency`, `NoLongerReplaces`, with `Display`); a fork release (or branch commit) without `replaces` is kept back with `NoLongerReplaces`; a diverged branch stays `UnchangedReason::Diverged`; verify in `crates/gitpm/tests/dependencies.rs`: `update_all_backtracks_to_compatible_versions_and_keeps_current_fallback` (constraints), `update_all_keeps_back_a_release_needing_a_missing_dependency_without_a_catalog`, `update_all_resolves_replacement_versions_together_with_consumers` (no longer replaces)

## 2. CLI

- [x] 2.1 Print `ID kept back at V (latest L: REASON)`, `ID updated A → B (latest L kept back: REASON)` and `ID skipped at V (dependency DEP is not installed)` from the plan; verify `update_all_reports_kept_back_packages_and_replace_back_without_a_replacement` (`mutating_commands_print_one_line_per_package_and_no_ok_lines` has no `update all` lines)

## 3. FFI

- [x] 3.1 `HeldBack.reason` (`HeldBackReason`) and `message`, `Unchanged.message`, `UnchangedReason::BrokenDependency`; verify `records_convert_field_by_field` and the Swift build of `scripts/build-apple.sh`

## 4. Specs, tests, docs

- [x] 4.1 Dagger: forced uninstall then `update all` with and without catalog (`uc_071:forced_removal_skips_broken_consumers`), new dependency without a catalog kept back (`uc_071:new_dependency_without_a_catalog_is_kept_back`); verify `./itests/dt --only scenarios`
- [x] 4.2 README `update all` section (dnf model, reasons) and "Changed in 0.6"; verify by reading against the updates spec
- [x] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate --all --strict`
