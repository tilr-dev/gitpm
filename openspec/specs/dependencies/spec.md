# dependencies Specification

## Purpose

Defines package-to-package dependencies: the requirement syntax, PubGrub resolution at install and update time, how missing dependencies are installed, and the guards that keep dependents working.

## Requirements

### Requirement: Requirement syntax
`dependencies` in `plugin.json` SHALL map package IDs to Cargo-style requirements: `1.2` (= `^1.2`), `^1.2.3`, `~1.2`, `>=1.3, <2`, `=1.0.0`, `1.*`, `*`. Pre-release comparators and unknown operators SHALL be refused with `invalid_manifest`. Every key MUST be a valid package ID (`invalid_id` otherwise) and a package MUST NOT depend on itself (`invalid_manifest`, "plugin ID depends on itself"). Verified by `deps::requirements_follow_cargo_semantics`.

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
Install SHALL resolve dependencies with PubGrub: installed packages stay at their versions; missing ones SHALL be installed from the configured catalog in the same transaction. A direct Git install without a catalog SHALL only accept already installed dependencies and SHALL name the missing one. Verified by `catalog_install_resolves_dependencies_and_records_edges`, `direct_git_install_uses_installed_plugins_or_the_catalog`, `direct_install_without_catalog_names_the_missing_dependency`.

#### Scenario: Dependency pulled from the catalog
- **WHEN** `app` requires `lib ^1` and only the catalog has `lib`
- **THEN** `install-catalog app` installs `lib` and `app` in one transaction

#### Scenario: No catalog
- **WHEN** a direct Git install needs `lib`, which is not installed, and no catalog is configured
- **THEN** install fails with `dependency_conflict` naming `lib` and nothing is installed

### Requirement: Conflicts are explained
Unsatisfiable dependencies SHALL fail with `dependency_conflict` carrying the solver's explanation that names the packages involved. Cycles between packages SHALL resolve normally. Verified by `deps::solver_explains_conflicts_naming_both_consumers` and `deps::solver_accepts_cycles_and_reports_missing_packages`.

#### Scenario: Two consumers disagree
- **WHEN** installed `a` requires `lib ^1` and the new package `b` requires `lib ^2`
- **THEN** install fails with `dependency_conflict` and the message names `a`, `b` and `lib`

### Requirement: Updates keep dependents working
Update and rollback SHALL refuse with `dependency_conflict` when the new version breaks an installed dependent; new dependencies of an update SHALL be installed in the same transaction. The joint solve of `update all` (kept-back packages, no downgrades) is specified in updates, and the dependency checks of `replace` (the provider's own dependencies must stay satisfiable, else `dependency_conflict`) in fork-replacement. Verified by `cycles_resolve_and_updates_keep_dependents_working`.

#### Scenario: Breaking update
- **WHEN** `app` requires `lib ^1` and the user runs `update lib` to 2.0.0
- **THEN** the update fails with `dependency_conflict` and lib stays at 1.x

### Requirement: Dependents guard removal
Single source for the `has_dependents` guard: `disable`, `forget` and `uninstall` of a package others depend on SHALL refuse with `has_dependents` (names in `dependents`) unless forced; `UninstallPlan.dependents` SHALL list them. Disabling an active replacement SHALL also refuse with `has_dependents`, naming the replaced originals. With force each SHALL be allowed. Verified by `dependents_guard_disable_forget_and_uninstall` and `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable an active replacement without force
- **WHEN** `me/greeter` replaces `acme/greeter` and the user runs `disable me/greeter`
- **THEN** the command fails with `has_dependents` naming `acme/greeter` and both `dist/` folders are unchanged

#### Scenario: Uninstall a dependency
- **WHEN** `app` depends on `lib` and the user runs `gitpm uninstall lib`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` lib is removed

### Requirement: Broken consumers are reported, not enforced
After a forced removal, already broken consumers SHALL be reported (in `list` dependency entries and `check` "Unsatisfied dependency" lines) but SHALL NOT block unrelated operations.

#### Scenario: Unrelated install after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, and the user installs `other`
- **THEN** the install succeeds and `list` reports `app`'s dependency on `lib` as unsatisfied

### Requirement: State keeps dependency edges
State SHALL keep each package's dependency edges (the `plugins.<id>.dependencies` field of the record in workspace-layout), and `list` SHALL report for each dependency its provider, version and whether it is satisfied, plus each package's `dependents`.

#### Scenario: list after install
- **WHEN** `app` requiring `lib ^1` is installed with `lib` 1.2.0
- **THEN** `list` shows `app`'s dependency on `lib` as satisfied by `lib` 1.2.0 and `lib`'s `dependents` contains `app`
