## MODIFIED Requirements

### Requirement: Branch mode follows the branch HEAD by commit
`update ID` in branch mode SHALL move to the HEAD of the recorded origin branch, by commit (no version bump needed), and refuse with `version_conflict` when its history diverged ("ID: history of BRANCH diverged; automatic update refused") or the branch is gone. An explicit `--ref BRANCH` SHALL skip the ancestor check (the way back after a force-push). Verified by `branch_mode_follows_origin_head_and_refuses_diverged_history` and `a_branch_gone_from_the_origin_is_reported_not_fatal`.

#### Scenario: Rewritten upstream
- **WHEN** the origin's branch was force-pushed so the installed commit is not an ancestor of its HEAD
- **THEN** `update ID` (and `update ID --dev`) fails with `version_conflict` whose message names ID and whose structured error carries ID, and the workspace is unchanged, while `update ID --ref BRANCH` moves to the new HEAD

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has any lost-work item (install-uninstall), staged files included. The complete library message SHALL name the package and describe the lost-work items; FFI SHALL preserve its ID and items, and CLI SHALL print that message unchanged. Both SHALL count a tag the origin had with the same object at the last fetch as published, not `UnpublishedTag`; rollback stays offline, while `update` also accepts a tag the origin has now. Verified by `local_work_blocks_update_and_rollback`, `update_and_rollback_refuse_hidden_local_work` and `rollback_of_a_fork_treats_fetched_upstream_tags_as_published`.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the clone's HEAD was moved away from the installed commit and the user runs `rollback ID`
- **THEN** rollback fails with `local_changes` listing `HeadMoved`

#### Scenario: Fork, update, rollback
- **WHEN** `me/hello`, a fork whose repository also holds its upstream's tags (for example `acme/hello@1.0.0`), is installed at 1.0.0, updated to 1.1.0, and the user runs `rollback me/hello`
- **THEN** the upstream tags fetched from the fork's origin count as published, as they did for the update, and rollback prints `me/hello rolled back 1.1.0 → 1.0.0` without a network access

#### Scenario: Retained upstream tag on rollback
- **WHEN** the clone of a fork keeps a tag of its upstream that no remote branch contains, and the fork's origin had that tag with the same object at the last fetch, and the user runs `rollback`
- **THEN** rollback works offline and counts the tag as published, as `update` does, so it does not fail with `local_changes` because of it

#### Scenario: Tag created locally
- **WHEN** a tag was created in the clone by hand and no fetch from the origin brought it
- **THEN** update and rollback alike fail with `local_changes` listing `UnpublishedTag`

#### Scenario: Stash blocks update
- **WHEN** the clone has a stash entry and the user runs `update ID`
- **THEN** update fails with `local_changes` and nothing changes

### Requirement: update all honors each package's mode and refuses local work
`update all` SHALL take no target: it SHALL solve release-mode packages over their release tags, offer each branch-mode package its branch HEAD as only candidate, list the others in `unchanged` with a reason (`pinned`, `diverged`, `branch_gone`), and refuse lost work (`local_changes`) in any clone without partially deploying; moved release tags never fail it (package-ids). Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_follows_each_mode`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all takes no --ref, --dev or --release; it follows each package's recorded mode` and changes nothing

#### Scenario: Mixed modes
- **WHEN** one package is in release mode with a newer release, one in branch mode whose branch moved, and one pinned
- **THEN** `update all` updates the first two and prints `C pinned at V (REF)` for the third

#### Scenario: Branch gone or diverged
- **WHEN** the branch a package follows was deleted on the origin, or force-pushed
- **THEN** `update all` updates the other packages and lists it in `unchanged` with reason `branch_gone` or `diverged`

#### Scenario: Unpublished local work identifies its clone
- **WHEN** a clone of `acme/greeter` has an unpublished local tag and `update all` is run
- **THEN** it fails with `local_changes`, whose library message names `acme/greeter` and the tag, and CLI prints that message unchanged
- **AND** FFI preserves `id` `acme/greeter` and the lost-work items, and the workspace stays unchanged
