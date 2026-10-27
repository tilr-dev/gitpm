## MODIFIED Requirements

### Requirement: Broken consumers are reported, not enforced
After a forced removal, already broken consumers SHALL be reported (in `list` dependency entries, `check` "Unsatisfied dependency" lines and as skipped in `update all`) but SHALL NOT block unrelated operations; `update all` SHALL leave them at their installed revision, out of its solve, and SHALL never install a dependency that was removed with force for them. Verified by `update_all_skips_broken_consumers_and_never_reinstalls`.

#### Scenario: Unrelated install after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, and the user installs `other`
- **THEN** the install succeeds and `list` reports `app`'s dependency on `lib` as unsatisfied

#### Scenario: update all after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, a catalog that has `lib` is configured (or none is), and the user runs `update all`
- **THEN** the other packages are updated, `app` is in `unchanged` with reason `broken_dependency` (`dependency acme/lib is not installed`), `lib` is not installed again, and `list` and `check` still report `app`'s dependency unsatisfied

### Requirement: Disabled packages and dependencies
A disabled package SHALL NOT satisfy a direct dependency of a package that is or becomes active in `install`, `install-catalog`, `update ID`, `enable`, `rollback` and `restore`: each SHALL fail with `dependency_conflict` ("DEP is disabled; enable it"), unless forced (`install --force`, `enable --force`). `update all`, `list` and `check` SHALL count a disabled provider as not satisfying it. Verified by `disabled_packages_satisfy_no_dependency` and the tests the scenarios name.

#### Scenario: Dependency on a disabled package in list
- **WHEN** `app` requires `lib ^1` and `lib` 1.2.0 is installed but disabled
- **THEN** `list` reports `app`'s dependency on `lib` as unsatisfied with the problem `lib is disabled`

#### Scenario: Install next to a disabled dependency
- **WHEN** `lib` 1.2.0 is installed but disabled and the user installs `app`, which requires `lib ^1`
- **THEN** the install fails with `dependency_conflict` naming `lib` as disabled and the workspace is unchanged

#### Scenario: Rollback and restore next to a disabled dependency
- **WHEN** `app` 1.1.0 (previous 1.0.0) requires `lib ^1`, `lib` was disabled with `--force`, and the user runs `rollback app`, or `forget app` and then `restore app`
- **THEN** both fail with `dependency_conflict` ("lib is disabled; enable it") and the workspace is unchanged (`rollback_and_restore_refuse_disabled_dependencies`)

#### Scenario: Inactive packages take disabled versions as given
- **WHEN** the user runs `download`, or `update`/`rollback`/`update all` of a disabled package, next to a disabled dependency
- **THEN** it succeeds, and dependencies it pulls from the catalog are recorded disabled (`update_of_a_disabled_package_pulls_disabled_dependencies`, `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`, `update_all_pulls_disabled_dependencies_for_a_disabled_consumer`)

#### Scenario: update all
- **WHEN** an enabled consumer's dependency is disabled and the user runs `update all`
- **THEN** the provider counts as not satisfying it: the consumer is skipped with reason `broken_dependency` (`dependency lib: lib is disabled`) and the run succeeds (`update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`)

#### Scenario: Only direct dependencies
- **WHEN** `app` requires `lib`, `lib` (enabled, forced) requires the disabled `base`
- **THEN** `install`, `enable`, `update`, `rollback` and `restore` of `app` succeed; `lib` is reported broken (`only_direct_dependencies_must_be_active`)
