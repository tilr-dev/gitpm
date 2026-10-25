## MODIFIED Requirements

### Requirement: Package validity is per package
Every package SHALL need a canonical UUID unique in the repository (package-ids, "Case-colliding IDs are refused") and a nonunique name, a manifest and a committed package output folder selected by its manifest dist (default dist/) (neither a symlink; the missing package dist message is in dist), and packages MUST NOT nest. Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it.

#### Scenario: Case variants
- **WHEN** two packages of the repository have IDs differing only in letter case
- **THEN** both folders are defective
- **AND** this is covered by `packages::ids_differing_only_in_case_make_both_folders_defective`

#### Scenario: Broken sibling
- **WHEN** `packages/theme` has no `dist/` and the user installs `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` from the same repository
- **THEN** the install succeeds

### Requirement: Keep-alive refs
Shared clones SHALL keep `refs/gitpm/<uuid>/current` and `.../previous` for every package that uses them (registered or forgotten), updated with each change and deleted with the package, so history rewrites on the origin, pruned tags or `git gc` never lose an installed or rollback revision. Verified by `installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc`.

#### Scenario: Upstream force-push and gc
- **WHEN** upstream rewrites history and the clone is garbage-collected
- **THEN** `rollback` of each package still finds its previous revision

### Requirement: Repository releases
All packages of a repository SHALL share one version and the single release tag `vVERSION` (package-ids, "Release tag names"). The package set SHALL be read from `gitpm.json` at the tagged commit so a package may move between releases. A tag whose commit lacks the selected package ID SHALL be ignored without a warning and SHALL NOT be a broken release of that package (package-ids). Legacy `ID@VERSION` tags and other non-release tags SHALL be ignored without a warning.

#### Scenario: Newest tag lacks its package
- **WHEN** the newest tag `v0.3.0` points to a commit without the selected package `76a9b591-60f3-5c7f-9e1c-31648fc73bfd`
- **THEN** it is ignored without warnings; install, update, check and catalog update consider only releases containing `76a9b591-60f3-5c7f-9e1c-31648fc73bfd`

#### Scenario: Independent releases
- **WHEN** `v0.2.0` releases both `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` and `f851f609-bb81-52d6-bfef-33798624782c` at 0.2.0, and the user updates only installed `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` with no dependency requiring an update of `f851f609-bb81-52d6-bfef-33798624782c`
- **THEN** `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` moves to 0.2.0 and `f851f609-bb81-52d6-bfef-33798624782c` keeps its installed revision

#### Scenario: vVERSION tag in a multi-package repository
- **WHEN** a multi-package repository has tag `v0.2.0` and every listed package has version 0.2.0 at its commit
- **THEN** it is a release of each listed package, and install, update and `catalog update` discover it

### Requirement: Choosing a package at install
`install REPO ID` (or `--package ID`) SHALL install one package. Without an ID a repository with several packages SHALL be refused with `invalid_argument` and the list of its packages, as SHALL an ID that is not a package of the repository; one with a single listed package needs none; a repository release tag passed with `--ref vVERSION` SHALL NOT select a package by itself. Verified by `installing_a_monorepo_without_a_package_id_lists_the_packages`.

#### Scenario: Missing package ID
- **WHEN** the user runs `gitpm install ./tilr-packages` on a repository with two packages
- **THEN** it fails with `invalid_argument` (`repository has 2 packages at commit …; choose one by ID: 76a9b591-60f3-5c7f-9e1c-31648fc73bfd (packages/store), f851f609-bb81-52d6-bfef-33798624782c (packages/theme)`)

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. The clone SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` and `f851f609-bb81-52d6-bfef-33798624782c` share a clone and `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` is uninstalled
- **THEN** the clone stays and `f851f609-bb81-52d6-bfef-33798624782c` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone with the usual local-work guards and refuses without changes if it holds local work
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

### Requirement: Shared clone HEAD handling
With one installed or forgotten user, HEAD SHALL count as moved when it differs from that user's recorded Git revision, even if tagged. With several users, HEAD SHALL count as moved only when it is neither a user's recorded current revision nor a tagged commit. Root and nested packages SHALL follow the same rule. Removal or forgetting SHALL move HEAD to a remaining installed package, preserving working-tree changes or refusing with `local_changes` on conflict.

#### Scenario: HEAD moves to a remaining package
- **WHEN** the clone's HEAD is at `76a9b591-60f3-5c7f-9e1c-31648fc73bfd`'s revision and `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` is uninstalled
- **THEN** HEAD moves to `f851f609-bb81-52d6-bfef-33798624782c`'s installed revision, keeping working-tree changes or refusing with `local_changes` if they conflict
- **AND** this is covered by `removing_a_package_moves_the_shared_head_to_a_remaining_one`

#### Scenario: Sole user's HEAD is at another tagged commit
- **WHEN** a root or nested package is the clone's only installed or forgotten user and HEAD differs from its recorded Git revision, even at another tagged commit
- **THEN** an operation guarding that clone refuses with `local_changes` and reports `HeadMoved`

#### Scenario: Retained clone uses the previous Git revision
- **WHEN** the clone's only user is a build retaining a previous Git installation
- **THEN** HEAD at that previous Git revision does not count as moved, even if its tag was removed
- **AND** HEAD at another commit counts as moved, even if that commit is tagged

#### Scenario: Several users permit a tagged HEAD
- **WHEN** a clone has several installed or forgotten users and HEAD is a user's recorded current revision or a tagged commit
- **THEN** HEAD does not count as moved

### Requirement: Forgotten packages of shared clones
Forgetting a package of a shared clone SHALL keep a `forgotten` record (format in workspace-layout) that still holds the clone; `restore` SHALL record it again under the rules of install-uninstall ("Restore preconditions") and deploy it when active; `uninstall` of the forgotten package SHALL remove the record and, if last, the clone. Verified by `a_forgotten_shared_package_keeps_its_clone_until_uninstalled`.

#### Scenario: Shared clone gone
- **WHEN** the shared clone of a forgotten package was deleted
- **THEN** `uninstall` drops the record, and `restore` fails with `repository_not_found` (the clone is what restore needs, as for any restore without a clone); the CLI adds a `Hint:` line naming `gitpm uninstall ID` and a new install
- **AND** this is covered by `a_forgotten_record_whose_clone_is_gone_can_be_uninstalled`

#### Scenario: Restore from shared clone
- **WHEN** `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` was forgotten and the user runs `restore 76a9b591-60f3-5c7f-9e1c-31648fc73bfd`
- **THEN** it is recorded in state at its recorded revision with its recorded `enabled` flag, and `dist/76a9b591-60f3-5c7f-9e1c-31648fc73bfd/` is deployed when it is enabled

### Requirement: Forking packages of one repository
`fork ID [ID...]` SHALL accept only installed IDs from one recorded source, else `invalid_argument` listing IDs and sources. It SHALL clone the whole repository, retain all files and the package layout, and assign every package a fresh UUID while preserving its name with `forkOf` and `replaces`. Dependencies between repository packages SHALL be rewritten. A root package SHALL stay at the root; no package subset SHALL be selected.

#### Scenario: Revisions on diverged histories
- **WHEN** the installed revisions of the forked packages are not ancestors of the newest one
- **THEN** the fork fails with `invalid_argument` ("… on diverged histories …") and creates nothing

#### Scenario: Package at an older revision
- **WHEN** one forked package is installed at an older revision than the fork starts from
- **THEN** its folder and submodule entries come from that revision in the cloned source

#### Scenario: Partial fork
- **WHEN** the user forks only installed `f851f609-bb81-52d6-bfef-33798624782c` of a three-package repository, where its sibling packages are not installed
- **THEN** all three packages are retained with fresh UUIDs and unchanged names, their internal dependencies are rewritten, and repository files and `gitpm.json` are preserved
- **AND** this is covered by `fork_ids_select_the_repository_and_rewrite_all_package_dependencies` and `packages_forked_together_must_come_from_one_repository`

### Requirement: Dependencies between forks are rewritten
Every dependency between packages of the source repository SHALL be rewritten to its fork; the requirement SHALL be kept when the fork's version satisfies it, else become `^V`, and each rewrite SHALL be printed. A manifest depending on both an original and its fork SHALL be refused with `invalid_manifest`.

#### Scenario: Requirement replaced
- **WHEN** the fork's version does not satisfy the original requirement
- **THEN** the output has `Dependency: P now depends on F ^V (was O REQ, which the fork's version does not satisfy)`

#### Scenario: Rewrite printed
- **WHEN** `955bf29f-2893-56ef-9439-369c50455ae7` depends on `f851f609-bb81-52d6-bfef-33798624782c ^1.0` and both are forked with fresh UUIDs
- **THEN** the output has `Dependency: 09e5894a-6e33-5a1e-91e0-730a2f8cb569 now depends on d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50 ^1.0 (was f851f609-bb81-52d6-bfef-33798624782c)`
- **AND** this is covered by `fork::a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order`

#### Scenario: Dependency cycle between forks
- **WHEN** packages forked together depend on each other in a cycle
- **THEN** fork rewrites the dependencies to their fork IDs without trying to order a release or installation
- **AND** the cycle alone does not trigger an implicit release, installation or replacement
