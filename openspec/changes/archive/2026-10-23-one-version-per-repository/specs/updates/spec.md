## MODIFIED Requirements

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has any lost-work item (install-uninstall), staged files included. The complete library message SHALL name the package and describe the lost-work items; FFI SHALL preserve its ID and items, and CLI SHALL print that message unchanged. Both SHALL count a tag the origin had with the same object at the last fetch as published, not `UnpublishedTag`; rollback stays offline, while `update` also accepts a tag the origin has now.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the clone's HEAD was moved away from the installed commit and the user runs `rollback ID`
- **THEN** rollback fails with `local_changes` listing `HeadMoved`
- Verified by `local_work_blocks_update_and_rollback`, `update_and_rollback_refuse_hidden_local_work` and `rollback_of_a_fork_treats_fetched_upstream_tags_as_published`.

#### Scenario: Fork, update, rollback
- **WHEN** `me/hello`, a fork whose repository also holds its upstream's tags (for example `v1.0.0` on a commit naming `acme/hello`), is installed at 1.0.0, updated to 1.1.0, and the user runs `rollback me/hello`
- **THEN** the upstream tags fetched from the fork's origin count as published, as they did for the update, and rollback prints `me/hello rolled back 1.1.0 → 1.0.0` without a network access

#### Scenario: Retained upstream tag on rollback
- **WHEN** the clone of a fork keeps a `vVERSION` tag of its upstream that no remote branch contains, and the fork's origin had that tag with the same object at the last fetch, and the user runs `rollback`
- **THEN** rollback works offline and counts the tag as published, as `update` does, so it does not fail with `local_changes` because of it

#### Scenario: Tag created locally
- **WHEN** a tag was created in the clone by hand and no fetch from the origin brought it
- **THEN** update and rollback alike fail with `local_changes` listing `UnpublishedTag`

#### Scenario: Stash blocks update
- **WHEN** the clone has a stash entry and the user runs `update ID`
- **THEN** update fails with `local_changes` and nothing changes

### Requirement: update follows the recorded mode
`update ID` without a target SHALL follow the package's recorded mode: release mode moves to the newest release, branch mode to the branch HEAD, and a pinned package SHALL stay unchanged with reason `pinned` (CLI: `ID pinned at V (REF)`). `--ref REF` and `--dev` SHALL move the package and record the new mode as at install (install-uninstall); `--release` SHALL return it to release mode at the newest release. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `acme/greeter` was installed with `--ref v0.1.0`, 0.2.0 is released and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter pinned at 0.1.0 (v0.1.0)` and nothing changes

#### Scenario: Back to releases
- **WHEN** the user then runs `update acme/greeter --release`
- **THEN** it prints `acme/greeter updated 0.1.0 → 0.2.0 (major)` and the record has no `mode`

### Requirement: An explicit return to releases may go down
`update ID --release` SHALL move to the newest release also when it is below the installed version; following releases (`update ID` in release mode) never goes down. Verified by `workspace::stable_update_never_downgrades`.

#### Scenario: Back to releases from above
- **WHEN** a package follows `main` at 0.4.0, the newest release is 0.3.0 and the user runs `update ID --release`
- **THEN** it moves down to 0.3.0 in release mode; a plain `update` of a release-mode package whose newest release is below the installed version (its release was retracted) stays up to date with the retraction warning ("Stable update never downgrades")

#### Scenario: Back to releases at the installed version
- **WHEN** a package is pinned to a commit with version 0.1.0, the release tag of 0.1.0 is another commit, and the user runs `update ID --release`
- **THEN** the installed commit is kept, the mode becomes release (a change), and the CLI warns `release tag v0.1.0 now points to COMMIT; the installed commit is kept`; this is covered by `return_to_releases_at_the_installed_version_keeps_the_commit`

### Requirement: A change of mode alone is a change
An update whose target resolves to the installed commit but records another mode SHALL write the new mode and report a change (`RevisionChange.changed`, `from` and `to` at the same commit; CLI `ID pinned at V (REF)`, `ID follows branch B at V`, `ID follows releases at V`). Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pin at the installed commit
- **WHEN** `acme/greeter` follows releases at 0.2.0 and the user runs `update acme/greeter --ref v0.2.0`
- **THEN** it prints `acme/greeter pinned at 0.2.0 (v0.2.0)` and the record says `{"pinned": "v0.2.0"}`

### Requirement: Protected packages update through their catalog
A protected package (recorded `catalog`) SHALL update only to versions its verified catalog lists, read from the recorded location whatever `Options.catalog` is; the release commit, package folder, manifest and file hashes SHALL match the record before any change (`catalog` otherwise). `update ID --detach` (`Manager::update_with`) SHALL drop the protection and update from the origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `acme/greeter` 0.2.0 has the tag `v0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update acme/greeter` prints `acme/greeter kept at 0.2.0 (upstream 0.3.0 not in catalog)` and `update all` lists it in `unchanged` with that reason
- **AND** a ref or branch target needs a detach: `update acme/greeter --ref v0.3.0` fails with `invalid_argument` ("… detach it to follow a ref or branch of its origin")

#### Scenario: Hash mismatch on update
- **WHEN** the verified catalog lists 0.3.0 but the files at the recorded commit do not match its hashes
- **THEN** `update acme/greeter` and `update all` fail with `catalog` ("catalog file hashes do not match git dist of acme/greeter@0.3.0: the release differs from the one added to the catalog") and the workspace is unchanged; a release commit the origin does not have fails with `catalog` ("… is not on the origin")

#### Scenario: Catalog no longer verified or unreachable
- **WHEN** the owner removed `"verified": true` from the catalog that protects `acme/greeter`, or the catalog cannot be read
- **THEN** `update acme/greeter` fails with `catalog` ("… is no longer verified; detach …") or `http` and nothing changes
- **AND** `update all` does not fail: it keeps `acme/greeter` in `unchanged` with the reason `catalog_unavailable` (`acme/greeter kept at 0.2.0 (catalog unavailable: REASON)`) and updates the other packages
- **AND** this is covered by `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`

#### Scenario: New dependency of a protected package
- **WHEN** protected `acme/app` 1.1.0 in its verified catalog adds a dependency `acme/lib` that is not installed, and no catalog or an unverified one is configured
- **THEN** `update acme/app` and `update all` install `acme/lib` from the recorded verified catalog and record it protected by that catalog
- **AND** this is covered by `workspace::new_dependencies_of_a_protected_package_come_from_its_catalog`

#### Scenario: Detach
- **WHEN** the user runs `update acme/greeter --detach`
- **THEN** it prints `acme/greeter detached from catalog LOCATION; it now follows its origin` and `acme/greeter updated 0.2.0 → 0.3.0 (major)`, the record loses `catalog` and later updates follow the origin
- **AND** a detach with no newer release is still a change (`workspace::detach_without_a_new_release_is_a_change`); detaching an unprotected package fails with `invalid_argument`; `update all --detach` is a usage error (exit status 1)

### Requirement: Pre-releases are opt-in
Install, update and `update all` SHALL never pick a pre-release implicitly. A pre-release SHALL be installed only when named (`--ref v1.3.0-beta.1`, `install-catalog ID --version 1.3.0-beta.1`, a dependency range naming it). Once one is installed in release mode, `update` and `update all` SHALL also consider newer pre-releases of the same MAJOR.MINOR.PATCH besides every newer release. A pre-release tag not considered never blocks an update. Verified by `workspace::pre_releases_are_opt_in`.

#### Scenario: Newest tag is a pre-release
- **WHEN** a package is at 1.2.0 and the origin also has `1.3.0-beta.1`
- **THEN** `update` reports `up to date (1.2.0)`

#### Scenario: A ref install is pinned
- **WHEN** the user installs with `--ref v1.3.0-beta.1` and later runs `update ID --release`
- **THEN** the package stays pinned until then; in release mode it follows the 1.3.0 track

#### Scenario: Only pre-release tags
- **WHEN** a repository has only the tag `v1.0.0-beta.1`
- **THEN** `install` fails with `no_releases` and `install SOURCE --ref v1.0.0-beta.1` installs it

#### Scenario: A consumer's range names a pre-release
- **WHEN** a consumer requires `lib ^1.3.0-beta.1`
- **THEN** a fresh install of `lib` for it takes `1.3.0-beta.1` (`dependency_ranges_pick_pre_releases_only_when_named`); `update all` never moves an installed release of `lib` onto a pre-release

#### Scenario: Following a beta
- **WHEN** `1.3.0-beta.1` is installed with `install-catalog ID --version 1.3.0-beta.1` and the origin has `1.3.0-beta.2` and `1.4.0-beta.1`, later also `1.3.0`
- **THEN** `update` moves to `1.3.0-beta.2`, then to `1.3.0`, and never to `1.4.0-beta.1`

### Requirement: Stable update never downgrades
An update that follows releases SHALL compare versions only and SHALL never downgrade; installed revisions are kept by commit (package-ids). A retracted, moved or broken installed release tag SHALL leave the package up to date with a warning, in `update ID` and `update all` alike. Verified by `stable_update_never_downgrades`, `update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases` and the tests the scenarios name.

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit on the origin and the user runs `update ID`
- **THEN** it prints `ID up to date (V)` and `Warning: release tag vV now points to COMMIT; the installed commit is kept`, and the workspace is unchanged

#### Scenario: Retracted release in update all
- **WHEN** the origin deleted the tag of the installed release and the user runs `update all`
- **THEN** the package is listed in `unchanged` with the retraction warning, as for `update ID`

#### Scenario: Warnings
- **WHEN** the installed release was retracted (its tag deleted, only older releases or none left), its tag moved, or its tag now points at a commit it is no release of (a broken tag, package-ids)
- **THEN** the warning is `installed release ID@V no longer exists on the origin`, `release tag vV now points to COMMIT; the installed commit is kept` or `release tag vV now points to COMMIT (skipped: REASON); the installed commit is kept`
- **AND** this is covered by `a_deleted_tag_in_a_shared_clone_is_reported_not_fatal` and `a_broken_installed_release_tag_is_named_as_skipped`

### Requirement: Explicit ref resolves against the origin
`update ID --ref REF` (and install) SHALL resolve REF by exact name: tag, fetched origin branch, commit ID (7 to 64 lowercase hex digits); `HEAD` SHALL mean the origin's default branch, recorded by name. Revision expressions and stale local branches SHALL be refused with `invalid_argument`. A tag or commit MAY go to an older version or keep the installed one elsewhere. Verified by `explicit_ref_to_the_moved_installed_tag_is_accepted` and `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** the package moves to that commit, pinned to it, and the workspace records the same version at the new commit

#### Scenario: Revision expression
- **WHEN** the user runs `install SOURCE --ref HEAD~1` or `update ID --ref main~1`
- **THEN** it fails with `invalid_argument` ("unknown ref …: not a tag, origin branch or commit") and nothing changes

#### Scenario: Branch name
- **WHEN** the origin's branch `feature` moved after install and the user runs `update ID --ref feature`
- **THEN** the package moves to the fetched `origin/feature` commit, not the clone's stale local branch, and is recorded in branch mode `feature`

#### Scenario: Moved release tag by ref
- **WHEN** the release tag of the installed version moved and the user runs `update ID --ref vV`
- **THEN** the package is pinned to the tag's new commit and the CLI warns `release tag vV now points to COMMIT`

### Requirement: Update mirrors the origin's tags
A single-package update of a package with its own clone SHALL mirror the origin's tags (pruned and force-fetched): a deleted release disappears from the clone, a moved one is seen at its new commit (package-ids). When the deleted tag is the installed (newest) release, `update ID` and `update all` report the package up to date with a warning ("Stable update never downgrades"). Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` prints `ID up to date (V)` with `Warning: installed release ID@V no longer exists on the origin`, changing nothing
