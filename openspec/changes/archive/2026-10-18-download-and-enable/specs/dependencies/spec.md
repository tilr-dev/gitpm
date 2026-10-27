## ADDED Requirements

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every dependency of ID that no active package satisfies (missing, disabled, replaced away or of a non-matching version) and SHALL NOT enable dependencies itself; only direct dependencies are checked. `enable ID --force` SHALL enable it anyway; the consumer is then reported broken ("Broken consumers are reported, not enforced"). An already enabled package is not checked. Verified by `enable_refuses_inactive_dependencies_unless_forced`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied

## MODIFIED Requirements

### Requirement: Disabled packages and dependencies
A disabled package SHALL NOT satisfy a dependency of a package that is or becomes active in `install`, `install-catalog`, `update ID`, `enable`, `rollback` and `restore`: each SHALL fail with `dependency_conflict` ("DEP is disabled; enable it") when a direct dependency of that package is provided only by a disabled package (the check of "enable needs active dependencies"; `install --force` and `enable --force` bypass it). `update all` and the `list`/`check` reports SHALL count a disabled provider as not satisfying it. Only `download`, whose packages stay disabled, and `update`/`rollback` of a disabled package MAY take the versions of disabled packages as given; dependencies an inactive package pulls from the catalog are recorded disabled. Verified by `disabled_packages_satisfy_no_dependency`, `rollback_and_restore_refuse_disabled_dependencies`, `only_direct_dependencies_must_be_active` and `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags` (the `update all` part).

#### Scenario: Dependency on a disabled package in list
- **WHEN** `app` requires `lib ^1` and `lib` 1.2.0 is installed but disabled
- **THEN** `list` reports `app`'s dependency on `lib` as unsatisfied with the problem `lib is disabled`

#### Scenario: Install next to a disabled dependency
- **WHEN** `lib` 1.2.0 is installed but disabled and the user installs `app`, which requires `lib ^1`
- **THEN** the install fails with `dependency_conflict` naming `lib` as disabled and the workspace is unchanged

#### Scenario: Rollback and restore next to a disabled dependency
- **WHEN** `app` 1.1.0 (previous 1.0.0) requires `lib ^1`, `lib` was disabled with `--force`, and the user runs `rollback app`, or `forget app` and then `restore app`
- **THEN** both fail with `dependency_conflict` ("lib is disabled; enable it") and the workspace is unchanged

#### Scenario: Only direct dependencies
- **WHEN** `app` requires `lib`, `lib` (enabled, forced) requires the disabled `base`
- **THEN** `install`, `enable`, `update`, `rollback` and `restore` of `app` succeed; `lib` is reported broken
