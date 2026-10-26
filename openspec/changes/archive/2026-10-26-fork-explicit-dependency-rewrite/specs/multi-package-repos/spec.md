## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only installed IDs from one recorded source without `--from`, or IDs from the explicit `--from SOURCE` repository, else `invalid_argument` listing IDs and sources. It SHALL clone the whole repository, retain all files and package layout, and give every package a fresh UUID, preserving its name with `forkOf` and `replaces`. Internal dependencies SHALL retain their original IDs and requirements until explicitly rewritten. No package subset SHALL be selected.

#### Scenario: Revisions on diverged histories
- **WHEN** the installed revisions of the forked packages are not ancestors of the newest one
- **THEN** the fork fails with `invalid_argument` ("… on diverged histories …") and creates nothing

#### Scenario: Package at an older revision
- **WHEN** one forked package is installed at an older revision than the fork starts from
- **THEN** its folder and submodule entries come from that revision in the cloned source

#### Scenario: Partial fork
- **WHEN** the user forks only installed `f851f609-bb81-52d6-bfef-33798624782c` of a three-package repository, where its sibling packages are not installed
- **THEN** all three packages are retained with fresh UUIDs and unchanged names, their internal dependencies are unchanged, and repository files and `gitpm.json` are preserved
- **AND** this is covered by `fork_ids_select_the_repository_and_preserve_dependencies_until_explicit_rewrite` and `packages_forked_together_must_come_from_one_repository`

#### Scenario: Source repository fork
- **WHEN** one ID of a multi-package repository is forked with `--from SOURCE`
- **THEN** every package at the resolved release or ref is retained with fresh UUIDs and preserved names; internal dependencies are unchanged and all `forkOf.commit` values equal the resolved commit

### Requirement: Dependencies between forks are rewritten
`fork_rewrite(DIR)` (FFI `forkRewrite`, CLI `fork-rewrite DIR`) SHALL explicitly rewrite internal fork dependencies using local `forkOf.id` values without a workspace, returning `DependencyRewrite` records. The CLI SHALL print each rewrite. Requirements SHALL be kept if satisfied by the fork version, else become `^V`. Conflicting dependencies SHALL fail with `invalid_manifest` before writing.

#### Scenario: Requirement replaced
- **WHEN** the explicit rewrite is called and the fork's version does not satisfy the original requirement
- **THEN** the output has `Dependency: P now depends on F ^V (was O REQ, which the fork's version does not satisfy)`

#### Scenario: Rewrite printed
- **WHEN** `955bf29f-2893-56ef-9439-369c50455ae7` depends on `f851f609-bb81-52d6-bfef-33798624782c ^1.0` and both have been forked with fresh UUIDs and the explicit rewrite is called
- **THEN** the output has `Dependency: 09e5894a-6e33-5a1e-91e0-730a2f8cb569 now depends on d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50 ^1.0 (was f851f609-bb81-52d6-bfef-33798624782c)`
- **AND** this is covered by `fork::a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order`

#### Scenario: Dependency cycle between forks
- **WHEN** packages forked together depend on each other in a cycle
- **THEN** the explicit rewrite rewrites the dependencies to their fork IDs without trying to order a release or installation
- **AND** the cycle alone does not trigger an implicit release, installation or replacement

#### Scenario: Repeat rewrite
- **WHEN** the explicit rewrite is called again without changing the fork manifests
- **THEN** it returns no rewrites and leaves the files unchanged

#### Scenario: Validation before writing
- **WHEN** any package has no forkOf, two packages declare the same forkOf.id, or a manifest depends on both an original and its fork
- **THEN** the call fails with `invalid_manifest` and no manifest is changed

#### Scenario: External dependencies
- **WHEN** a fork manifest depends on a package whose original is outside the fork repository
- **THEN** the explicit rewrite preserves that dependency and its requirement

#### Scenario: Invalid rewritten manifest
- **WHEN** a dependency on a package's own original would become a dependency on itself
- **THEN** the explicit call fails with `invalid_manifest` and no manifest is changed
