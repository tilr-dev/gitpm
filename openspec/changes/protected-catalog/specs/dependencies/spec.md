## MODIFIED Requirements

### Requirement: Install resolves dependencies
Install and single-package update SHALL resolve dependencies with PubGrub: installed packages stay at their versions; missing ones SHALL be installed from the configured catalog in the same transaction, each recorded with that catalog's location only when it is verified (catalog, "One catalog per workspace"). Single source for the no-catalog case: without a catalog only installed packages SHALL satisfy dependencies, and a missing one SHALL fail with `dependency_conflict` naming it.

#### Scenario: Dependency pulled from the catalog
- **WHEN** `app` requires `lib ^1` and only the catalog has `lib`
- **THEN** `install-catalog app` installs `lib` and `app` in one transaction
- **AND** `lib` is recorded with the catalog's location when the catalog is verified, and without one otherwise; this is covered by `catalog_install_resolves_dependencies_and_records_edges` and `direct_git_install_uses_installed_plugins_or_the_catalog`

#### Scenario: No catalog
- **WHEN** a direct Git install (or an update to a release) needs `lib`, which is not installed, and no catalog is configured
- **THEN** it fails with `dependency_conflict` naming `lib` and the workspace is unchanged
- **AND** the message notes "no catalog configured"; `update all` keeps such a package back instead (updates); this is covered by `direct_install_without_catalog_names_the_missing_dependency`

