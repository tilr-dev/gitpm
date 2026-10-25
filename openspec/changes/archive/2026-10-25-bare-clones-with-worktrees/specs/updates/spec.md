## MODIFIED Requirements

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse lost work in the affected worktree, including staged files, with `local_changes`. The library message SHALL name the package and items; FFI SHALL preserve them and CLI print the message unchanged. Tags with the origin's object at the last fetch count as published; update also accepts currently published tags. Rollback stays offline.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the package worktree's HEAD was moved away from the installed commit and the user runs `rollback ID`
- **THEN** rollback fails with `local_changes` listing `HeadMoved`
- Verified by `local_work_blocks_update_and_rollback`, `update_and_rollback_refuse_hidden_local_work` and `rollback_of_a_fork_treats_fetched_upstream_tags_as_published`.

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
