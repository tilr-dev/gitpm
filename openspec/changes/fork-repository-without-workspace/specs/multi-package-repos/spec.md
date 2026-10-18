## MODIFIED Requirements

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only IDs of one repository: all installed from the same source, or all packages of the `--from SOURCE` repository (else `invalid_argument` listing each ID with its source). A new fork SHALL start at the newest installed revision of the forked packages (with `--from`: the resolved release or ref), SHALL remove the folders of unforked packages and list exactly the forked folders in `gitpm.json`, in one commit `Fork A, B as me/A, me/B` made without running hooks; local edits of an installed clone stay uncommitted. A root package SHALL stay the root package of its fork.

#### Scenario: Revisions on diverged histories
- **WHEN** the installed revisions of the forked packages are not ancestors of the newest one
- **THEN** the fork fails with `invalid_argument` ("… on diverged histories …") and creates nothing

#### Scenario: Package at an older revision
- **WHEN** one forked package is installed at an older revision than the fork starts from
- **THEN** its folder comes from that revision; local edits of that folder in the clone make the fork fail with `local_changes`

#### Scenario: Partial fork
- **WHEN** the user forks `tilr/theme tilr/panel` of a three-package repository
- **THEN** the fork contains only `packages/theme` and `packages/panel` and prints `Only the forked packages are left in the tree and in gitpm.json.`
- **AND** this is covered by `fork_of_two_packages_keeps_exactly_those_and_rewrites_their_dependency` and `packages_forked_together_must_come_from_one_repository`

#### Scenario: ID not in the --from repository
- **WHEN** the user runs `gitpm fork tilr/none --from ./tilr-plugins --ns me --into x`
- **THEN** it fails with `invalid_argument` naming the packages of the repository and creates nothing
