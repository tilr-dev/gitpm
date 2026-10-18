# dependencies Specification

## Purpose

Defines package-to-package dependencies: the requirement syntax, PubGrub resolution at install and update time, how missing dependencies are installed, and the guards that keep dependents working.

## Requirements

### Requirement: Requirement syntax
`dependencies` in the manifest SHALL map package IDs to Cargo-style requirements: `1.2` (= `^1.2`), `^1.2.3`, `~1.2`, `>=1.3, <2`, `=1.0.0`, `1.*`, `*`. Pre-release comparators and unknown operators SHALL be refused with `invalid_manifest`. Every key MUST be a valid package ID (`invalid_id` otherwise) and a package MUST NOT depend on itself (`invalid_manifest`, "package ID depends on itself"). Verified by `deps::requirements_follow_cargo_semantics`.

#### Scenario: Caret on 0.x
- **WHEN** a dependency requires `^0.2`
- **THEN** 0.2.5 satisfies it and 0.3.0 does not

#### Scenario: Self-dependency
- **WHEN** `acme/app`'s manifest lists `acme/app` in `dependencies`
- **THEN** the manifest is refused with `invalid_manifest`

#### Scenario: Pre-release requirement
- **WHEN** a dependency requires `>=1.0.0-beta`
- **THEN** the manifest is refused with `invalid_manifest` ("pre-release versions are not supported")

### Requirement: Install resolves dependencies
Install and single-package update SHALL resolve dependencies with PubGrub: installed packages stay at their versions; missing ones SHALL be installed from the configured catalog in the same transaction, each recorded with that catalog's canonical location (`packages.<id>.catalog`; one catalog per workspace as in catalog). Single source for the no-catalog case: without a catalog only installed packages SHALL satisfy dependencies, and a missing one SHALL fail with `dependency_conflict` naming it.

#### Scenario: Dependency pulled from the catalog
- **WHEN** `app` requires `lib ^1` and only the catalog has `lib`
- **THEN** `install-catalog app` installs `lib` and `app` in one transaction
- **AND** `lib` is recorded with the catalog's location; this is covered by `catalog_install_resolves_dependencies_and_records_edges` and `direct_git_install_uses_installed_plugins_or_the_catalog`

#### Scenario: No catalog
- **WHEN** a direct Git install (or an update to a release) needs `lib`, which is not installed, and no catalog is configured
- **THEN** it fails with `dependency_conflict` naming `lib` and the workspace is unchanged
- **AND** the message notes "no catalog configured"; `update all` keeps such a package back instead (updates); this is covered by `direct_install_without_catalog_names_the_missing_dependency`

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

### Requirement: Conflicts are explained
Unsatisfiable dependencies SHALL fail with `dependency_conflict` carrying the solver's explanation that names the packages involved. Cycles between installed packages SHALL resolve normally; only `fork --replace` refuses a dependency cycle between the packages it forks (multi-package-repos, "Dependencies between forks are rewritten"). Verified by `deps::solver_explains_conflicts_naming_both_consumers` and `deps::solver_accepts_cycles_and_reports_missing_packages`.

#### Scenario: Two consumers disagree
- **WHEN** installed `a` requires `lib ^1` and the new package `b` requires `lib ^2`
- **THEN** install fails with `dependency_conflict` and the message names `a`, `b` and `lib`

### Requirement: Updates keep dependents working
Update and rollback SHALL refuse with `dependency_conflict` when the new version breaks an installed dependent; new dependencies of an update SHALL be installed in the same transaction. The joint solve of `update all` (kept-back packages, no downgrades) is specified in updates, and the dependency checks of `replace` (the replacing package's own dependencies must stay satisfiable, else `dependency_conflict`) in fork-replacement. Verified by `cycles_resolve_and_updates_keep_dependents_working`.

#### Scenario: Breaking update
- **WHEN** `app` requires `lib ^1` and the user runs `update lib` to 2.0.0
- **THEN** the update fails with `dependency_conflict` and lib stays at 1.x

### Requirement: Dependents guard removal
Single source for the `has_dependents` guard: `disable`, `forget` and `uninstall` of a package SHALL refuse with `has_dependents` unless forced when an installed package's dependency resolves to it, directly or through a replacement binding (the consumers of ORIGINAL count as dependents of FORK); `dependents` and `UninstallPlan.dependents` SHALL name those consumers. With force each SHALL be allowed. Verified by `dependents_guard_disable_forget_and_uninstall`.

#### Scenario: Uninstall a replacing package with consumers
- **WHEN** `app` depends on `acme/greeter`, which `me/greeter` replaces, and the user runs `uninstall me/greeter`
- **THEN** it fails with `has_dependents` naming `app`

#### Scenario: Forget a replacing package without consumers
- **WHEN** `me/greeter` replaces `acme/greeter`, nothing depends on `acme/greeter`, and the user runs `forget me/greeter`
- **THEN** it succeeds without force and drops the binding (fork-replacement, "Active fork rules")

#### Scenario: Disable an active replacement without force
- **WHEN** `me/greeter` replaces `acme/greeter` and the user runs `disable me/greeter`
- **THEN** the command fails with `has_dependents` naming `acme/greeter` and both deployed folders are unchanged
- **AND** disabling an active replacing package is refused even when no consumer is affected; `dependents` then names the replaced originals; this is covered by `disable_removes_and_enable_restores_also_around_a_replacement`

#### Scenario: Uninstall a dependency
- **WHEN** `app` depends on `lib` and the user runs `gitpm uninstall lib`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` lib is removed

### Requirement: Broken consumers are reported, not enforced
After a forced removal, already broken consumers SHALL be reported (in `list` dependency entries, `check` "Unsatisfied dependency" lines and as skipped in `update all`) but SHALL NOT block unrelated operations; `update all` SHALL leave them at their installed revision, out of its solve, and SHALL never install a dependency that was removed with force for them. Verified by `update_all_skips_broken_consumers_and_never_reinstalls`.

#### Scenario: Unrelated install after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, and the user installs `other`
- **THEN** the install succeeds and `list` reports `app`'s dependency on `lib` as unsatisfied

#### Scenario: update all after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, a catalog that has `lib` is configured (or none is), and the user runs `update all`
- **THEN** the other packages are updated, `app` is in `unchanged` with reason `broken_dependency` (`dependency acme/lib is not installed`), `lib` is not installed again, and `list` and `check` still report `app`'s dependency unsatisfied

### Requirement: State keeps dependency edges
State SHALL keep each package's dependency edges (the `packages.<id>.dependencies` field of the record in workspace-layout), and `list` SHALL report for each dependency the package that provides it (the replacing package when ORIGINAL is bound), its version and whether it is satisfied, plus each package's `dependents`.

#### Scenario: list after install
- **WHEN** `app` requiring `lib ^1` is installed with `lib` 1.2.0
- **THEN** `list` shows `app`'s dependency on `lib` as satisfied by `lib` 1.2.0 and `lib`'s `dependents` contains `app`

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every dependency of ID that no active package satisfies (missing, disabled, replaced away or of a non-matching version) and SHALL NOT enable dependencies itself; only direct dependencies are checked. `enable ID --force` SHALL enable it anyway; the consumer is then reported broken ("Broken consumers are reported, not enforced"). An already enabled package is not checked. Verified by `enable_refuses_inactive_dependencies_unless_forced`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied
