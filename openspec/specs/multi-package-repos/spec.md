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
Every package SHALL need a `namespace/name` ID unique in the repository in any letter case (package-ids, "Case-colliding IDs are refused"), a manifest and a committed package output folder selected by its manifest dist (default dist/) (neither a symlink; the missing package dist message is in dist), and packages MUST NOT nest. Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it.

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
- **THEN** it is no release of any package and install, update and `catalog update` print no warning about it
- **AND** this is covered by `a_v_version_tag_in_a_multi_package_repository_is_ignored_without_a_warning`

### Requirement: Choosing a package at install
`install REPO ID` (or `--package ID`) SHALL install one package. Without an ID a repository with several packages SHALL be refused with `invalid_argument` and the list of its packages, as SHALL an ID that is not a package of the repository; one with a single listed package needs none; an `--ref ID@VERSION` tag names the package itself. Verified by `installing_a_monorepo_without_a_package_id_lists_the_packages`.

#### Scenario: Missing package ID
- **WHEN** the user runs `gitpm install ./tilr-packages` on a repository with two packages
- **THEN** it fails with `invalid_argument` (`repository has 2 packages at commit …; choose one by ID: tilr/store (packages/store), tilr/theme (packages/theme)`)

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share a bare repository at `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. Each Git package SHALL have its own detached worktree at its recorded current commit. The bare repository SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone without checking out or moving a sibling worktree; only the affected package worktree is guarded
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

#### Scenario: Independent package revisions
- **WHEN** two packages from one source are installed at different commits and one is updated or rolled back
- **THEN** each worktree HEAD matches its own recorded commit, and the other worktree and its files remain unchanged

### Requirement: Shared clone HEAD handling
A Git package worktree SHALL be clean and its HEAD SHALL equal the package's recorded current commit before update, rollback, uninstall or restore, regardless of root or nested layout and regardless of other packages or tags. Lost work SHALL produce `local_changes` under the existing confirmation rules for uninstall. The bare repository SHALL never be checked out; its HEAD SHALL NOT govern a package's local-work check. Removing or forgetting one package SHALL NOT move another worktree's HEAD.

#### Scenario: Sole user's HEAD is at another tagged commit
- **WHEN** the affected worktree HEAD differs from its recorded commit, even at another package revision or a tagged commit
- **THEN** update, rollback and unconfirmed uninstall refuse with `local_changes` listing `HeadMoved`

#### Scenario: Dirty package worktree
- **WHEN** a root or nested Git package worktree contains staged, unstaged or untracked changes
- **THEN** update, rollback and unconfirmed uninstall refuse with `local_changes` for that package

#### Scenario: HEAD moves to a remaining package
- **WHEN** one package from a shared bare repository is removed or forgotten
- **THEN** remaining worktrees retain their own HEAD and bytes

#### Scenario: Retained clone uses the previous Git revision
- **WHEN** a build retains a previous Git revision for offline rollback
- **THEN** the bare repository protects that revision with a keep-alive ref and no package worktree is retained; bare HEAD does not participate in the local-work guard

#### Scenario: Several users permit a tagged HEAD
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
