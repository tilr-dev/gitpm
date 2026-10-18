# Tasks

## 1. Library

- [ ] 1.1 Only active packages satisfy dependencies in install, enable and single update; `dependency_conflict` "DEP is disabled; enable it"; verify `disabled_packages_satisfy_no_dependency`
- [ ] 1.2 `Manager::download`/`download_from_catalog`: record disabled, pull missing dependencies disabled, deploy nothing; `install` = download + enable in one transaction; verify `workspace::download_records_disabled_packages_and_dependencies` and `catalog_install_resolves_dependencies_and_records_edges`
- [ ] 1.3 `enable_with(id, force)`: refuse inactive dependencies unless forced; verify `enable_refuses_inactive_dependencies_unless_forced`

## 2. CLI

- [ ] 2.1 `download`, `download-catalog`, `enable --force`; `downloaded ID@V from SOURCE (disabled)` lines; usage blocks; verify `tests::every_command_has_a_usage_block` and `mutating_commands_print_one_line_per_package_and_no_ok_lines`

## 3. FFI

- [ ] 3.1 `download`, `downloadFromCatalog`, `enable(force)`; verify `records_convert_field_by_field` and `manager_round_trip_on_empty_workspace`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: download → enable order, forced enable, install next to a disabled dependency; verify `./itests/dt --only scenarios`
- [ ] 4.2 README commands and "Changed in 0.6" (BREAKING: disabled packages satisfy nothing, strict `enable`); verify by reading the install/enable sections against the specs
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
