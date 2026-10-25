## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only installed IDs from one recorded source without `--from`, or IDs from the explicit `--from SOURCE` repository, else `invalid_argument` listing IDs and sources. It SHALL clone the whole repository, retain all files and the package layout, and rename every package into the new namespace with `forkOf` and `replaces`. Dependencies between repository packages SHALL be rewritten. A root package SHALL stay at the root; no package subset SHALL be selected.

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

#### Scenario: Source repository fork
- **WHEN** one ID of a multi-package repository is forked with `--from SOURCE`
- **THEN** every package at the resolved release or ref is retained and renamed; internal dependencies are rewritten and all `forkOf.commit` values equal the resolved commit
