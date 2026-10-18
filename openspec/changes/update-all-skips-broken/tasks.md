# Tasks

## 1. Library

- [ ] 1.1 `update all` roots = healthy packages; broken consumers kept back with reason; removed dependencies never reinstalled; no `dependency_conflict` for a broken state; verify `update_all_skips_broken_consumers_and_never_reinstalls` (with and without a catalog)
- [ ] 1.2 `HeldBack.reason` with the five reasons; fork release without `replaces` and diverged branch kept back; verify unit tests per reason in `upgrade.rs`

## 2. CLI

- [ ] 2.1 Print `ID kept back at V (latest L: REASON)` from the plan; verify `mutating_commands_print_one_line_per_package_and_no_ok_lines` with a non-default reason

## 3. FFI

- [ ] 3.1 `HeldBack.reason`; verify `records_convert_field_by_field`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: forced uninstall then `update all` with and without catalog; verify `./itests/dt --only scenarios`
- [ ] 4.2 README `update all` section (dnf model, reasons) and "Changed in 0.6"; verify by reading against the updates spec
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
