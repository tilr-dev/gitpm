## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only installed IDs from one recorded source without `--from`, or IDs from the explicit `--from SOURCE` repository, else `invalid_argument` listing IDs and sources. It SHALL clone the whole repository, retain all files and package layout, and give every package a fresh UUID, preserving its name with `forkOf` and `replaces`. Internal dependencies SHALL retain their original IDs and requirements until explicitly edited by the caller. No package subset SHALL be selected.

#### Scenario: Revisions on diverged histories
- **WHEN** the installed revisions of the forked packages are not ancestors of the newest one
- **THEN** the fork fails with `invalid_argument` ("… on diverged histories …") and creates nothing

#### Scenario: Package at an older revision
- **WHEN** one forked package is installed at an older revision than the fork starts from
- **THEN** its folder and submodule entries come from that revision in the cloned source

#### Scenario: Partial fork
- **WHEN** the user forks only installed `f851f609-bb81-52d6-bfef-33798624782c` of a three-package repository, where its sibling packages are not installed
- **THEN** all three packages are retained with fresh UUIDs and unchanged names, their internal dependencies are unchanged, and repository files and `gitpm.json` are preserved
- **AND** this is covered by `fork_ids_select_the_repository_and_preserve_dependencies_until_explicit_edit` and `packages_forked_together_must_come_from_one_repository`

#### Scenario: Source repository fork
- **WHEN** one ID of a multi-package repository is forked with `--from SOURCE`
- **THEN** every package at the resolved release or ref is retained with fresh UUIDs and preserved names; internal dependencies are unchanged and all `forkOf.commit` values equal the resolved commit

## REMOVED Requirements

### Requirement: Dependencies between forks are rewritten
**Reason**: Callers choose explicit dependency IDs and requirements instead of inferring them from provenance.
**Migration**: Inspect packages and add, update or remove dependencies through the package dependency API.
