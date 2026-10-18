## ADDED Requirements

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every dependency of ID that no active package satisfies (missing, disabled, replaced away or of a non-matching version) and SHALL NOT enable dependencies itself. `enable ID --force` SHALL enable it anyway; the consumer is then reported broken ("Broken consumers are reported, not enforced"). Verified by `enable_refuses_inactive_dependencies_unless_forced`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied

## MODIFIED Requirements

### Requirement: Disabled packages and dependencies
A disabled package SHALL NOT satisfy a dependency anywhere: `install`, `enable` and single-package `update` of an enabled package SHALL fail with `dependency_conflict` ("DEP is disabled; enable it") when a dependency resolves only to a disabled package, and `update all`, `list` and `check` SHALL report it unsatisfied. Only `download`, whose packages stay disabled, MAY take the versions of disabled packages as given. Verified by `disabled_packages_satisfy_no_dependency`.

#### Scenario: Dependency on a disabled package in list
- **WHEN** `app` requires `lib ^1` and `lib` 1.2.0 is installed but disabled
- **THEN** `list` reports `app`'s dependency on `lib` as unsatisfied with the problem `lib is disabled`

#### Scenario: Install next to a disabled dependency
- **WHEN** `lib` 1.2.0 is installed but disabled and the user installs `app`, which requires `lib ^1`
- **THEN** the install fails with `dependency_conflict` naming `lib` as disabled and the workspace is unchanged
