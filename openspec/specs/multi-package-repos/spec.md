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
Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it.

Verified by `workspace::a_defective_sibling_does_not_block_a_package`.

#### Scenario: Broken sibling
- **WHEN** `packages/theme` has no `dist/` and the user installs `tilr/store` from the same repository
- **THEN** the install succeeds

### Requirement: Repository releases
All packages of a repository SHALL share one version and the single release tag `vVERSION` (package-ids, "Release tag names"). The package set SHALL be read from `gitpm.json` at the tagged commit so a package may move between releases. A tag whose commit lacks the selected package ID SHALL be ignored without a warning and SHALL NOT be a broken release of that package (package-ids). Legacy `ID@VERSION` tags and other non-release tags SHALL be ignored without a warning.

#### Scenario: Newest tag lacks its package
- **WHEN** the newest tag `v0.3.0` points to a commit without the selected package `tilr/store`
- **THEN** it is ignored without warnings; install, update, check and catalog update consider only releases containing `tilr/store`

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

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share a bare repository at `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. The bare repository SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

Verified by `two_packages_share_one_clone_and_follow_repository_releases`.

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone without checking out or moving a sibling worktree; only the affected package worktree is guarded
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

#### Scenario: Sibling install preserves an unpublished colliding tag
- **WHEN** installing another package would fetch an origin tag over a different local tag holding unpublished work
- **THEN** install refuses with `local_changes`, preserving the local tag, package worktrees and state

### Requirement: Shared clone HEAD handling
A Git package worktree SHALL be clean with HEAD at its recorded current commit before update, rollback, uninstall or restore, regardless of layout or siblings. Lost work SHALL yield `local_changes` under uninstall confirmation rules. Bare HEAD SHALL NOT govern this check. Removing or forgetting one package SHALL NOT move another worktree's HEAD. Shared refs SHALL count as lost work only when removing the bare repository.

#### Scenario: Moved worktree HEAD refuses even at a tagged commit
- **WHEN** the affected worktree HEAD differs from its recorded commit, even at another package revision or a tagged commit
- **THEN** update, rollback and unconfirmed uninstall refuse with `local_changes` listing `HeadMoved`

#### Scenario: Shared refs survive one package removal
- **WHEN** a clean worktree at its recorded commit is uninstalled while another package retains the bare repository
- **THEN** surviving shared refs require no confirmation, and only HEAD commits not retained by refs other than the removed package's keep-alive refs are listed as unpublished commits

#### Scenario: Dirty package worktree
- **WHEN** a root or nested Git package worktree contains staged, unstaged or untracked changes
- **THEN** update, rollback and unconfirmed uninstall refuse with `local_changes` for that package

#### Scenario: Removing a package preserves remaining worktree HEADs
- **WHEN** one package from a shared bare repository is removed or forgotten
- **THEN** remaining worktrees retain their own HEAD and bytes

#### Scenario: Retained clone uses the previous Git revision
- **WHEN** a build retains a previous Git revision for offline rollback
- **THEN** the bare repository protects that revision with a keep-alive ref and no package worktree is retained; bare HEAD does not participate in the local-work guard

#### Scenario: Sibling revisions do not exempt a moved worktree HEAD
- **WHEN** a source has several package worktrees and one worktree HEAD moves to another tagged commit
- **THEN** operations guarding that package refuse with `local_changes` listing `HeadMoved`, even when another package records that commit

### Requirement: Keep-alive refs
Bare source repositories SHALL keep `refs/gitpm/<namespace>/<name>/current` and `.../previous` for every package that uses them (registered or forgotten, including retained Git current/previous revisions across build transitions), updated with each change and deleted with the package, so history rewrites on the origin, pruned tags or `git gc` never lose an installed or rollback revision. Verified by `installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc`.

#### Scenario: Upstream force-push and gc
- **WHEN** upstream rewrites history and the clone is garbage-collected
- **THEN** `rollback` of each package still finds its previous revision

### Requirement: Forgotten packages of shared clones
Forgetting a package of a shared clone SHALL keep a `forgotten` record (format in workspace-layout) that still holds its bare repository and Git worktree; `restore` SHALL record it again under the rules of install-uninstall ("Restore preconditions") and deploy it when active; `uninstall` of the forgotten package SHALL remove the record its worktree and, if last, the bare repository. Verified by `a_forgotten_shared_package_keeps_its_clone_until_uninstalled`.

#### Scenario: Shared clone gone
- **WHEN** the shared clone of a forgotten package was deleted
- **THEN** `uninstall` drops the record, and `restore` fails with `repository_not_found` (the clone is what restore needs, as for any restore without a clone); the CLI adds a `Hint:` line naming `gitpm uninstall ID` and a new install
- **AND** this is covered by `a_forgotten_record_whose_clone_is_gone_can_be_uninstalled`

#### Scenario: Restore from shared clone
- **WHEN** `tilr/store` was forgotten and the user runs `restore tilr/store`
- **THEN** it is recorded in state at its recorded revision with its recorded `enabled` flag, and `dist/tilr/store/` is deployed when it is enabled

### Requirement: Root package that moved into a folder
A root package SHALL keep its source-keyed `repos/<key>` bare repository and its own worktree when a later release moves it into a package folder (`path` set, `clone` unchanged) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store` and preserves its `clone` key and worktree identity, so other packages from the same source reuse the clone

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

#### Scenario: Requirement replaced
- **WHEN** the fork's version does not satisfy the original requirement
- **THEN** the output has `Dependency: P now depends on F ^V (was O REQ, which the fork's version does not satisfy)`

#### Scenario: Rewrite printed
- **WHEN** `tilr/panel` depends on `tilr/theme ^1.0` and both are forked into `me`
- **THEN** the output has `Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)`
- **AND** this is covered by `fork::a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order`

#### Scenario: Dependency cycle between forks
- **WHEN** packages forked together depend on each other in a cycle
- **THEN** fork rewrites the dependencies to their fork IDs without trying to order a release or installation
- **AND** the cycle alone does not trigger an implicit release, installation or replacement
