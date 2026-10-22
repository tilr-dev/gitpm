## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only packages installed from one recorded source, else `invalid_argument` listing IDs and sources. It SHALL start at the newest installed revision of all packages from that source, restore every installed package folder from that source at its installed revision, include every package listed by the repository, and preserve all other repository files and the package layout. Uninstalled packages SHALL come from the newest installed revision of packages from that source. Every package SHALL receive a new namespace, `forkOf` and `replaces`, including uninstalled siblings at the starting revision; no package subset SHALL be selected. One commit `Fork A, B as me/A, me/B` SHALL record the result without user hooks. A root package SHALL remain at the root.

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
