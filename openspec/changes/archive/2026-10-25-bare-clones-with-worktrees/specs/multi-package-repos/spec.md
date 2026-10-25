## MODIFIED Requirements

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

#### Scenario: Sibling install preserves an unpublished colliding tag
- **WHEN** installing another package would fetch an origin tag over a different local tag holding unpublished work
- **THEN** install refuses with `local_changes`, preserving the local tag, package worktrees and state

#### Scenario: Independent package revisions
- **WHEN** two packages from one source are installed at different commits and one is updated or rolled back
- **THEN** each worktree HEAD matches its own recorded commit, and the other worktree and its files remain unchanged


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

### Requirement: Keep-alive refs
Bare source repositories SHALL keep `refs/gitpm/<namespace>/<name>/current` and `.../previous` for every package that uses them (registered or forgotten, including retained Git current/previous revisions across build transitions), updated with each change and deleted with the package, so history rewrites on the origin, pruned tags or `git gc` never lose an installed or rollback revision. Verified by `installed_revisions_of_a_shared_clone_survive_upstream_rewrites_and_gc`.

#### Scenario: Upstream force-push and gc
- **WHEN** upstream rewrites history and the clone is garbage-collected
- **THEN** `rollback` of each package still finds its previous revision
