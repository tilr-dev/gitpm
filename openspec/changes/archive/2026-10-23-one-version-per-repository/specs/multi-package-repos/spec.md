## RENAMED Requirements

- FROM: `### Requirement: Per-package releases`
- TO: `### Requirement: Repository releases`

## MODIFIED Requirements

### Requirement: Repository releases
All packages of a repository SHALL share one version and the single release tag `vVERSION` (package-ids, "Release tag names"). The package set SHALL be read from `gitpm.json` at the tagged commit so a package may move between releases. A tag whose commit lacks a selected package SHALL be skipped as a broken tag (package-ids). Legacy `ID@VERSION` tags and other non-release tags SHALL be ignored without a warning.

#### Scenario: Newest tag lacks its package
- **WHEN** the newest tag `v0.3.0` points to a commit without the selected package `tilr/store`
- **THEN** it is skipped ("tag NAME points to commit … without package ID") and, being the newest, makes selecting the latest release fail with `version_conflict` as package-ids specifies; an older such tag only produces the warning

#### Scenario: Independent releases
- **WHEN** `v0.2.0` releases both `tilr/store` and `tilr/theme` at 0.2.0, and the user updates only installed `tilr/store` with no dependency requiring an update of `tilr/theme`
- **THEN** `tilr/store` moves to 0.2.0 and `tilr/theme` keeps its installed revision

#### Scenario: vVERSION tag in a multi-package repository
- **WHEN** a multi-package repository has tag `v0.2.0` and every listed package has version 0.2.0 at its commit
- **THEN** it is a release of each listed package, and install, update and `catalog update` discover it

### Requirement: Choosing a package at install
`install REPO ID` (or `--package ID`) SHALL install one package. Without an ID a repository with several packages SHALL be refused with `invalid_argument` and the list of its packages, as SHALL an ID that is not a package of the repository; one with a single listed package needs none; a repository release tag passed with `--ref vVERSION` SHALL NOT select a package by itself. Verified by `installing_a_monorepo_without_a_package_id_lists_the_packages`.

#### Scenario: Missing package ID
- **WHEN** the user runs `gitpm install ./tilr-packages` on a repository with two packages
- **THEN** it fails with `invalid_argument` (`repository has 2 packages at commit …; choose one by ID: tilr/store (packages/store), tilr/theme (packages/theme)`)
