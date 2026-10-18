## MODIFIED Requirements

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

### Requirement: State keeps dependency edges
State SHALL keep each package's dependency edges (the `packages.<id>.dependencies` field of the record in workspace-layout), and `list` SHALL report for each dependency the package that provides it (the replacing package when ORIGINAL is bound), its version and whether it is satisfied, plus each package's `dependents`.

#### Scenario: list after install
- **WHEN** `app` requiring `lib ^1` is installed with `lib` 1.2.0
- **THEN** `list` shows `app`'s dependency on `lib` as satisfied by `lib` 1.2.0 and `lib`'s `dependents` contains `app`
