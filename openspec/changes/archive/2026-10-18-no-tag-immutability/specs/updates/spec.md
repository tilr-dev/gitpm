## RENAMED Requirements

- FROM: `### Requirement: Stable update never downgrades or accepts mutated releases`
- TO: `### Requirement: Stable update never downgrades`

## MODIFIED Requirements

### Requirement: Explicit ref resolves against the origin
`update ID --ref REF` (and install) SHALL resolve REF by exact name: tag, fetched origin branch, commit ID (7 to 64 lowercase hex digits); `HEAD` SHALL mean the origin's default branch, recorded by name. Revision expressions and local branches (`refs/heads/*`, stale) SHALL be refused with `invalid_argument`. A tag or commit MAY go to an older version or keep the installed version at another commit (tags are not policed, package-ids); the moved release tag of the installed version is named in the warning `release tag ID@V now points to COMMIT`. Verified by `explicit_ref_to_the_moved_installed_tag_is_accepted` and `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** the package moves to that commit, pinned to it, and the workspace records the same version at the new commit

#### Scenario: Revision expression
- **WHEN** the user runs `install SOURCE --ref HEAD~1` or `update ID --ref main~1`
- **THEN** it fails with `invalid_argument` ("unknown ref …: not a tag, origin branch or commit") and nothing changes

#### Scenario: Branch name
- **WHEN** the origin's branch `feature` moved after install and the user runs `update ID --ref feature`
- **THEN** the package moves to the fetched `origin/feature` commit, not the clone's stale local branch, and is recorded in branch mode `feature`

### Requirement: Stable update never downgrades
An update that follows releases SHALL compare versions only and SHALL never downgrade; installed revisions are kept by commit (package-ids, "Tags are not policed; integrity via protected catalog"). When the installed release was retracted (its tag deleted on the origin) and only older releases or none remain, the package SHALL stay and be reported up to date with the warning `installed release ID@V no longer exists on the origin`; when the tag of the installed version moved, it SHALL stay up to date with the warning `release tag ID@V now points to COMMIT; the installed commit is kept`, and when that tag now points at a commit it is no release of (a broken tag, package-ids), with `release tag ID@V now points to COMMIT (skipped: REASON); the installed commit is kept`. `update ID` and `update all` SHALL behave alike. Verified by `stable_update_never_downgrades`, `update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases`, `a_deleted_tag_in_a_shared_clone_is_reported_not_fatal` and `a_broken_installed_release_tag_is_named_as_skipped`.

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit on the origin and the user runs `update ID`
- **THEN** it prints `ID up to date (V)` and `Warning: release tag ID@V now points to COMMIT; the installed commit is kept`, and the workspace is unchanged

#### Scenario: Retracted release in update all
- **WHEN** the origin deleted the tag of the installed release and the user runs `update all`
- **THEN** the package is listed in `unchanged` with the retraction warning, as for `update ID`

### Requirement: Update mirrors the origin's tags
A single-package update of a package with its own clone SHALL mirror the origin's tags (pruned and force-fetched): a release deleted on the origin disappears from the clone and a moved one is seen at its new commit (package-ids, "Tags are not policed; integrity via protected catalog"). When the deleted tag is the installed (newest) release, `update ID` and `update all` report the package up to date with a warning ("Stable update never downgrades"). Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` prints `ID up to date (V)` with `Warning: installed release ID@V no longer exists on the origin`, changing nothing

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has any lost-work item (list in install-uninstall, "Uninstall is planned and confirmed"), staged files included. Both SHALL classify tags the same way: a tag that the origin had with the same object at the last fetch is published, not `UnpublishedTag`; rollback stays offline and uses the tags of that last fetch, and `update`, which fetches anyway, also accepts a tag the origin has now with the same object. Verified by `local_work_blocks_update_and_rollback`, `update_and_rollback_refuse_hidden_local_work` and `rollback_of_a_fork_treats_fetched_upstream_tags_as_published`.

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

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` offline (no fetch) and redeploy; without a previous revision it SHALL fail with `no_previous_version`, also when the previous commit is no longer in the clone (pruned after the origin deleted or moved its tag), then naming ID and the commit. It SHALL print `ID rolled back A → B`. Like update, it is refused with `dependency_conflict` when the target version breaks an installed dependent (see dependencies, "Updates keep dependents working"). Verified by `rollback_to_a_pruned_commit_names_it`.

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

#### Scenario: Previous commit gone
- **WHEN** the previous revision's commit is no longer in the clone (pruned after the origin deleted its tag)
- **THEN** rollback fails with `no_previous_version` ("previous revision COMMIT of ID is no longer available") and the workspace is unchanged

### Requirement: update all never removes or downgrades
`update all` SHALL NOT remove or downgrade packages, SHALL preserve replacement bindings and disabled flags, SHALL solve only healthy packages and SHALL report each kept-back package with its latest version and `HeldBack.reason`; a broken state SHALL NOT fail the run. Verified by `update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases`, `update_all_skips_broken_consumers_and_never_reinstalls` and the tests the scenarios name.

#### Scenario: Kept back
- **WHEN** the newest release of a package does not fit a dependent's requirement
- **THEN** the CLI prints `ID kept back at V (latest L: dependency constraints)`, or, when it advances to an intermediate compatible release, `ID updated A → B (latest L kept back: dependency constraints)`, the reason taken from `HeldBack.reason`
- **AND** a package whose newer release needs a dependency that is not installed, with no catalog configured, is kept back with the reason `needs DEP REQ, which is not installed and no catalog is configured`

#### Scenario: Reasons
- **WHEN** a package is kept back
- **THEN** `HeldBack.reason` is `constraints` (`dependency constraints`), `missing_dependency` (`needs DEP REQ, which is not installed and no catalog is configured`) or `no_longer_replaces` (`no longer replaces ORIGINAL`, a replacing package whose newest release or branch commit no longer declares `replaces`)
- **AND** a diverged branch history stays an `unchanged` reason (`diverged`), not a kept-back one

#### Scenario: Broken state does not fail the run
- **WHEN** a consumer's dependency was removed with force and the other packages have newer releases
- **THEN** the broken consumer (a direct dependency unsatisfied now, as `list` reports it; an inactive package takes a disabled provider of a matching version as given) stays at its installed revision and its unsatisfied dependencies stay out of the solve, so they are never installed for it
- **AND** `update all` updates the others, lists the consumer in `unchanged` with reason `broken_dependency` (CLI `app skipped at V (dependency DEP is not installed)`) and exits 0, with or without a catalog; a single `update` of that consumer without a catalog still fails with `dependency_conflict` (dependencies)
- **AND** this is covered by `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags` for a disabled dependency
- **AND** `broken_dependency` takes precedence over the mode reasons (`pinned`, `diverged`, `branch_gone`)

#### Scenario: Dependencies pulled for a disabled consumer
- **WHEN** a disabled package's newer release needs a package that is not installed, and the user runs `update all`
- **THEN** the dependency is installed from the catalog recorded disabled and not deployed, and it takes disabled providers as given; a new dependency an active package pulls is enabled and needs active providers as `update ID` does (`update_all_pulls_disabled_dependencies_for_a_disabled_consumer`, `update_all_enables_dependencies_of_active_consumers_only`)

### Requirement: update all honors each package's mode and refuses local work
`update all` SHALL take no target: it SHALL solve release-mode packages over their release tags, offer each branch-mode package its branch HEAD as only candidate, list the others in `unchanged` with a reason (`pinned`, `diverged`, `branch_gone`), and refuse lost work (`local_changes`) in any clone without partially deploying; moved release tags do not fail it (package-ids, "Tags are not policed; integrity via protected catalog"). Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_follows_each_mode`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all takes no --ref, --dev or --release; it follows each package's recorded mode` and changes nothing

#### Scenario: Mixed modes
- **WHEN** one package is in release mode with a newer release, one in branch mode whose branch moved, and one pinned
- **THEN** `update all` updates the first two and prints `C pinned at V (REF)` for the third

#### Scenario: Branch gone or diverged
- **WHEN** the branch a package follows was deleted on the origin, or force-pushed
- **THEN** `update all` updates the other packages and lists it in `unchanged` with reason `branch_gone` or `diverged`

### Requirement: An explicit return to releases may go down
`update ID --release` SHALL move to the newest release also when it is below the installed version; following releases (`update ID` in release mode) never goes down. Verified by `workspace::stable_update_never_downgrades`.

#### Scenario: Back to releases from above
- **WHEN** a package follows `main` at 0.4.0, the newest release is 0.3.0 and the user runs `update ID --release`
- **THEN** it moves down to 0.3.0 in release mode; a plain `update` of a release-mode package whose newest release is below the installed version (its release was retracted) stays up to date with the retraction warning ("Stable update never downgrades")

#### Scenario: Back to releases at the installed version
- **WHEN** a package is pinned to a commit with version 0.1.0, the release tag of 0.1.0 is another commit, and the user runs `update ID --release`
- **THEN** the installed commit is kept, the mode becomes release (a change), and the CLI warns `release tag ID@0.1.0 now points to COMMIT; the installed commit is kept`; this is covered by `return_to_releases_at_the_installed_version_keeps_the_commit`
