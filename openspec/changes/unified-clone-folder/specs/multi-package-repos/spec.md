## MODIFIED Requirements

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. The clone SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone with the usual local-work guards and refuses without changes if it holds local work
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

### Requirement: Root package that moved into a folder
A root package SHALL keep its source-keyed `repos/<key>` clone when a later release moves it into a package folder (`path` set, `clone` unchanged) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store` and preserves its `clone` key, so other packages from the same source reuse the clone

### Requirement: Shared clone HEAD handling
For a clone with exactly one installed or forgotten user, HEAD SHALL count as moved when it differs from that user's recorded Git revision, even if HEAD is tagged. For a build retaining a Git clone, the recorded Git revision SHALL be its previous revision when that revision is a Git installation. With several installed or forgotten users, HEAD SHALL count as moved only when it is neither a recorded current revision of one of those users nor a tagged commit. These rules SHALL apply equally to root and nested packages. When a package is uninstalled or forgotten and another installed package remains, HEAD SHALL move to that remaining package's installed revision, keeping working-tree changes or refusing with `local_changes` if they conflict. Verified by `removing_a_package_moves_the_shared_head_to_a_remaining_one`.

#### Scenario: HEAD moves to a remaining package
- **WHEN** the clone's HEAD is at `tilr/store`'s revision and `tilr/store` is uninstalled
- **THEN** HEAD moves to `tilr/theme`'s installed revision

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
