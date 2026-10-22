## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only installed IDs from one recorded source, else `invalid_argument` listing IDs and sources. It SHALL clone the whole repository, retain all files and the package layout, and rename every package into the new namespace with `forkOf` and `replaces`. Dependencies between repository packages SHALL be rewritten. A root package SHALL stay at the root; no package subset SHALL be selected.

#### Scenario: Revisions on diverged histories
- **WHEN** the installed revisions of the forked packages are not ancestors of the newest one
- **THEN** the fork fails with `invalid_argument` ("… on diverged histories …") and creates nothing

#### Scenario: Package at an older revision
- **WHEN** one forked package is installed at an older revision than the fork starts from
- **THEN** its folder and submodule entries come from that revision in the cloned source

#### Scenario: Partial fork
- **WHEN** the user forks only installed `tilr/theme` of a three-package repository, where its sibling packages are not installed
- **THEN** all three packages are retained and renamed into the new namespace, their internal dependencies are rewritten, and repository files and `gitpm.json` are preserved
- **AND** this is covered by `fork_ids_select_the_repository_and_rewrite_all_package_dependencies` and `packages_forked_together_must_come_from_one_repository`

### Requirement: Dependencies between forks are rewritten
Every dependency between packages of the source repository SHALL be rewritten to its fork; the requirement SHALL be kept when the fork's version satisfies it, else become `^V`, and each rewrite SHALL be printed. A manifest depending on both an original and its fork SHALL be refused with `invalid_manifest`.

#### Scenario: Dependency cycle between forks
- **WHEN** packages forked together with `--replace` depend on each other in a cycle
- **THEN** the fork fails with `dependency_conflict` before anything is created

#### Scenario: Requirement replaced
- **WHEN** the fork's version does not satisfy the original requirement
- **THEN** the output has `Dependency: P now depends on F ^V (was O REQ, which the fork's version does not satisfy)`

#### Scenario: Rewrite printed
- **WHEN** `tilr/panel` depends on `tilr/theme ^1.0` and both are forked into `me`
- **THEN** the output has `Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)`
- **AND** this is covered by `fork::a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order` and `a_dependency_cycle_between_forks_is_refused_before_anything_is_created`

## REMOVED Requirements

### Requirement: Adding packages to an existing fork
**Reason**: A fork is a clone of one recorded source into a new repository; adding exported folders loses Git structure.
**Migration**: Fork the entire source repository into a new folder.
