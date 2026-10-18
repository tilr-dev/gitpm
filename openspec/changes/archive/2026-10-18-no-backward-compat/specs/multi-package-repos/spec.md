## MODIFIED Requirements

### Requirement: Per-package releases
Releases SHALL be per package with tags `ID@VERSION` (package-ids, "Release tag names"), and the package set SHALL be read from `gitpm.json` at the tagged commit so a package may move between releases. A tag whose commit lacks its package SHALL be skipped as a broken tag (package-ids). Other tags, `vVERSION` included, are not releases and SHALL be ignored without a warning. Verified by `a_package_can_move_to_another_folder_between_releases`.

#### Scenario: Newest tag lacks its package
- **WHEN** the newest tag `tilr/store@0.3.0` points to a commit without `tilr/store`
- **THEN** it is skipped ("tag NAME points to commit … without package ID") and, being the newest, makes selecting the latest release fail with `version_conflict` as package-ids specifies; an older such tag only produces the warning
- **AND** this is covered by `a_tag_on_a_commit_without_its_package_is_a_clear_error`

#### Scenario: Independent releases
- **WHEN** `tilr/store@0.2.0` is tagged while `tilr/theme` stays at 0.1.0
- **THEN** `update tilr/store` moves it to 0.2.0 and `tilr/theme` is untouched

#### Scenario: vVERSION tag in a multi-package repository
- **WHEN** a multi-package repository also has the tag `v0.2.0`
- **THEN** it is no release of any package and install, update and `catalog-update` print no warning about it
- **AND** this is covered by `a_v_version_tag_in_a_multi_package_repository_is_ignored_without_a_warning`

### Requirement: Choosing a package at install
`install REPO ID` (or `--package ID`) SHALL install one package. Without an ID a repository with several packages SHALL be refused with `invalid_argument` and the list of its packages, as SHALL an ID that is not a package of the repository; one with a single listed package needs none; an `--ref ID@VERSION` tag names the package itself. Verified by `installing_a_monorepo_without_a_package_id_lists_the_packages`.

#### Scenario: Missing package ID
- **WHEN** the user runs `gitpm install ./tilr-packages` on a repository with two packages
- **THEN** it fails with `invalid_argument` (`repository has 2 packages at commit …; choose one by ID: tilr/store (packages/store), tilr/theme (packages/theme)`)

### Requirement: Root package that moved into a folder
A root package SHALL keep its own `packages/<id>` clone when a later release moves it into a package folder (`path` set, `clone` absent) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store`

### Requirement: Adding packages to an existing fork
`fork … --into DIR` where `DIR` has `gitpm.json` SHALL add the package folders (files of the installed revisions, plain files only) at their original folders, in one commit of only those paths; `gitpm.json` and changed manifests MUST have no uncommitted changes, a Git identity is required (authoring, "Git identity is never invented in existing repositories"), and a failed commit SHALL restore the repository. Every refusal below SHALL be `invalid_argument` and change nothing.

#### Scenario: Folder or ID already there
- **WHEN** the fork repository already has the folder, or a package whose ID differs only in letter case
- **THEN** the add is refused

#### Scenario: Nesting folders
- **WHEN** an added folder would contain or be contained in another package folder
- **THEN** the add is refused ("package folder … would nest with …")

#### Scenario: Root package
- **WHEN** a forked package is the root package of its repository
- **THEN** the add is refused ("… is the root package of its repository …; fork it into a new folder")

#### Scenario: Add later
- **WHEN** the user runs `gitpm fork tilr/dock --ns me --into forks/me-packages` on an existing fork
- **THEN** it prints `Packages added to: …` and `me/dock forked from tilr/dock (packages/dock)`
- **AND** this is covered by `packages_are_added_to_an_existing_fork_with_dependencies_rewritten_both_ways` and `adding_to_a_fork_refuses_a_failing_commit_and_a_missing_identity_untouched`
