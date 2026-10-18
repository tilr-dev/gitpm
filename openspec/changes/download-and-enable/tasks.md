# Tasks

## 1. Library

- [x] 1.1 Only active packages satisfy dependencies in install, enable and single update; `dependency_conflict` "DEP is disabled; enable it"; verify `disabled_packages_satisfy_no_dependency`
- [x] 1.2 `Manager::download`/`download_from_catalog`: record disabled, pull missing dependencies disabled, deploy nothing; `install` = download + enable in one transaction; verify `workspace::download_records_disabled_packages_and_dependencies` and `catalog_install_resolves_dependencies_and_records_edges`
- [x] 1.3 `enable_with(id, force)`: refuse inactive dependencies unless forced; verify `enable_refuses_inactive_dependencies_unless_forced`

## 2. CLI

- [x] 2.1 `download`, `download-catalog`, `enable --force`; `downloaded ID@V from SOURCE (disabled)` lines; usage blocks; verify `tests::every_command_has_a_usage_block` and `mutating_commands_print_one_line_per_package_and_no_ok_lines`

## 3. FFI

- [x] 3.1 `download`, `downloadFromCatalog`, `enable(force)`; verify `records_convert_field_by_field` and `manager_round_trip_on_empty_workspace`

## 4. Specs, tests, docs

- [x] 4.1 Dagger: download → enable order, forced enable, install next to a disabled dependency; verify `./itests/dt --only scenarios`
- [x] 4.2 README commands and "Changed in 0.6" (BREAKING: disabled packages satisfy nothing, strict `enable`); verify by reading the install/enable sections against the specs
- [x] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`

## 5. TILR-36 decisions missed by the first draft

- [x] 5.1 `install --force`/`install-catalog --force` (`Manager::install_package_with`/`install_from_catalog_with`, FFI `install(force)`/`installFromCatalog(force)`): download plus a forced enable of the named package only; verify `forced_install_and_install_over_a_download` and `download_then_enable_through_the_cli`
- [x] 5.2 `install` over a download from the same source enables it (dependency check, `--force` bypass), `InstallResult.enabled` true, `already_installed` only when enabled; verify `forced_install_and_install_over_a_download`
- [x] 5.3 `rollback` of an active package and `restore` of an enabled record refuse disabled dependencies; verify `rollback_and_restore_refuse_disabled_dependencies` and `./itests/dt --only scenarios --filter uc_058`
- [x] 5.4 One direct active-dependency check in `deps.rs` for install, install over a download, `update ID`, `enable`, `rollback`, `restore`; dependencies pulled by an inactive package's update stay disabled; install over a download reports `available`/`requested_mode` and warns about a kept mode; verify `only_direct_dependencies_must_be_active`, `update_of_a_disabled_package_pulls_disabled_dependencies`, `install_over_a_download_reports_the_requested_mode`, FFI `download_from_catalog_then_enable`
