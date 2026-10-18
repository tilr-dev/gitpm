# multi-package-repos Specification

## Purpose

Defines multi-package repositories: the `gitpm.json` package list at the repository root, per-package releases, one shared clone per source in the workspace, keep-alive refs, forgotten records, and multi-package forks with dependency rewriting.

## Requirements

### Requirement: gitpm.json lists package folders
A `gitpm.json` `{"packages": [PATTERNS]}` at the repository root SHALL list package folders as relative `/`-separated patterns where `*` matches within one segment. Invalid patterns and unknown keys SHALL be refused with `invalid_manifest`; `*` SHALL never match a hidden folder. Without `gitpm.json` the repository root SHALL be the only package (the root package); with it, a manifest at the repository root is not a package. Verified by `packages::patterns_match_segment_by_segment`.

#### Scenario: Invalid pattern
- **WHEN** a pattern contains `**`, `..`, a backslash, an empty segment, a segment starting with `.`, or is absolute
- **THEN** reading `gitpm.json` fails with `invalid_manifest`
- **AND** this is covered by `packages::invalid_package_sets_are_refused`

#### Scenario: Glob pattern
- **WHEN** `gitpm.json` is `{"packages": ["packages/*"]}` and `packages/store`, `packages/theme`, `packages/.cache` exist
- **THEN** the packages are `packages/store` and `packages/theme`

### Requirement: Package validity is per package
Every package SHALL need a `namespace/name` ID unique in the repository in any letter case (package-ids, "Case-colliding IDs are refused"), a manifest and a committed package dist (neither a symlink; the missing package dist message is in dist), and packages MUST NOT nest. Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it. Verified by `workspace::a_defective_sibling_does_not_block_a_package`.

#### Scenario: Case variants
- **WHEN** two packages of the repository have IDs differing only in letter case
- **THEN** both folders are defective
- **AND** this is covered by `packages::ids_differing_only_in_case_make_both_folders_defective`

#### Scenario: Broken sibling
- **WHEN** `packages/theme` has no `dist/` and the user installs `tilr/store` from the same repository
- **THEN** the install succeeds

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

### Requirement: One shared clone per source
Packages of one source SHALL share one clone `repos/<name>-<hash of the source>`; state SHALL record `clone` and `path` per package and only `<path>/dist` SHALL be deployed. Single source for removing a shared clone: it SHALL be removed with the last package (recorded or forgotten) that uses it, with its lost work guarded then. Fetching into it takes the origin's tags as they are (package-ids). Verified by `two_packages_of_one_repository_share_one_clone_and_update_independently`.

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

### Requirement: Shared clone HEAD handling
HEAD of a shared clone SHALL count as moved only when it is neither an installed revision of one of its packages nor a tagged commit; when a package is uninstalled or forgotten, HEAD SHALL move to the installed revision of a remaining one, keeping working-tree changes or refusing with `local_changes` if they conflict. Verified by `removing_a_package_moves_the_shared_head_to_a_remaining_one`.

#### Scenario: HEAD moves to a remaining package
- **WHEN** the clone's HEAD is at `tilr/store`'s revision and `tilr/store` is uninstalled
- **THEN** HEAD moves to `tilr/theme`'s installed revision

### Requirement: Keep-alive refs
Shared clones SHALL keep `refs/gitpm/<namespace>/<name>/current` and `.../previous` for every package that uses them (registered or forgotten), updated with each change and deleted with the package, so history rewrites on the origin, pruned tags or `git gc` never lose an installed or rollback revision. Verified by `installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc`.

#### Scenario: Upstream force-push and gc
- **WHEN** upstream rewrites history and the clone is garbage-collected
- **THEN** `rollback` of each package still finds its previous revision

### Requirement: Forgotten packages of shared clones
Forgetting a package of a shared clone SHALL keep a `forgotten` record (format in workspace-layout) that still holds the clone; `restore` SHALL record it again under the rules of install-uninstall ("Restore preconditions") and deploy it when active; `uninstall` of the forgotten package SHALL remove the record and, if last, the clone. Verified by `a_forgotten_shared_package_keeps_its_clone_until_uninstalled`.

#### Scenario: Shared clone gone
- **WHEN** the shared clone of a forgotten package was deleted
- **THEN** `uninstall` drops the record, and `restore` fails with `repository_not_found` (the clone is what restore needs, as for any restore without a clone); the CLI adds a `Hint:` line naming `gitpm uninstall ID` and a new install
- **AND** this is covered by `a_forgotten_record_whose_clone_is_gone_can_be_uninstalled`

#### Scenario: Restore from shared clone
- **WHEN** `tilr/store` was forgotten and the user runs `restore tilr/store`
- **THEN** it is recorded in state at its recorded revision with its recorded `enabled` flag, and `dist/tilr/store/` is deployed when it is enabled

### Requirement: Root package that moved into a folder
A root package SHALL keep its own `packages/<id>` clone when a later release moves it into a package folder (`path` set, `clone` absent) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store`

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only IDs installed from the same repository (else `invalid_argument` listing each ID with its source). A new fork SHALL start at the newest installed revision of the forked packages, SHALL remove the folders of unforked packages from the tree and list exactly the forked folders in `gitpm.json`, in one commit `Fork A, B as me/A, me/B` made without running hooks; local edits of the clone stay uncommitted. A root package SHALL stay the root package of its fork.

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

### Requirement: Dependencies between forks are rewritten
A dependency on a package forked in the same command, or on an original that a package already in the fork repository forks, SHALL be rewritten to the fork, in both directions when adding; the requirement SHALL be kept when the fork's version satisfies it, else become `^V`, and each rewrite SHALL be printed. A manifest depending on both an original and its fork SHALL be refused with `invalid_manifest`.

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
