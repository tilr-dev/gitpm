# updates Specification

## Purpose

Defines how installed packages move between revisions: single-package `update` (by the recorded mode, `--ref`, `--dev`, `--release`), `rollback`, the joint `update all` upgrade, and the rules that protect local work and published releases.

## Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the origin and move to the newest release tag, across major versions and passing over pre-releases ("Major updates are marked", "Pre-releases are opt-in"). When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (same version, another commit: `ID updated V (old → new)`). Failures change nothing (errors). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update acme/greeter` twice
- **THEN** the first prints `acme/greeter updated 0.1.0 → 0.2.0 (major)`, the second `acme/greeter up to date (0.2.0)`

### Requirement: Updates need no catalog
`update ID` (in any mode) and `update all` SHALL read releases and branches from the origin of the package's clone (set from `packages.<id>.source`) and SHALL NOT read a catalog for unprotected packages, so they work without one (protected packages: "Protected packages update through their catalog"); `check` reads the same origin for packages the catalog does not list (catalog). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `acme/greeter` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update acme/greeter` prints `acme/greeter updated 0.1.0 → 0.2.0 (major)`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `acme/greeter would be updated 0.1.0 → 0.2.0 (major)` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `acme/greeter` says `update available` with `0.2.0 (major)` from the origin, and nothing in the workspace changes

### Requirement: Stable update never downgrades
An update that follows releases SHALL compare versions only and SHALL never downgrade; installed revisions are kept by commit (package-ids). A retracted, moved or broken installed release tag SHALL leave the package up to date with a warning, in `update ID` and `update all` alike. Verified by `stable_update_never_downgrades`, `update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases` and the tests the scenarios name.

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit on the origin and the user runs `update ID`
- **THEN** it prints `ID up to date (V)` and `Warning: release tag ID@V now points to COMMIT; the installed commit is kept`, and the workspace is unchanged

#### Scenario: Retracted release in update all
- **WHEN** the origin deleted the tag of the installed release and the user runs `update all`
- **THEN** the package is listed in `unchanged` with the retraction warning, as for `update ID`

#### Scenario: Warnings
- **WHEN** the installed release was retracted (its tag deleted, only older releases or none left), its tag moved, or its tag now points at a commit it is no release of (a broken tag, package-ids)
- **THEN** the warning is `installed release ID@V no longer exists on the origin`, `release tag ID@V now points to COMMIT; the installed commit is kept` or `release tag ID@V now points to COMMIT (skipped: REASON); the installed commit is kept`
- **AND** this is covered by `a_deleted_tag_in_a_shared_clone_is_reported_not_fatal` and `a_broken_installed_release_tag_is_named_as_skipped`

### Requirement: Update mirrors the origin's tags
A single-package update of a package with its own clone SHALL mirror the origin's tags (pruned and force-fetched): a deleted release disappears from the clone, a moved one is seen at its new commit (package-ids). When the deleted tag is the installed (newest) release, `update ID` and `update all` report the package up to date with a warning ("Stable update never downgrades"). Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` prints `ID up to date (V)` with `Warning: installed release ID@V no longer exists on the origin`, changing nothing

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
- **WHEN** the release tag of the installed version moved and the user runs `update ID --ref ID@V`
- **THEN** the package is pinned to the tag's new commit and the CLI warns `release tag ID@V now points to COMMIT`

### Requirement: Branch mode follows the branch HEAD by commit
`update ID` in branch mode SHALL move to the HEAD of the recorded origin branch, by commit (no version bump needed), and refuse with `version_conflict` when its history diverged ("ID: history of BRANCH diverged; automatic update refused") or the branch is gone. An explicit `--ref BRANCH` SHALL skip the ancestor check (the way back after a force-push). Verified by `branch_mode_follows_origin_head_and_refuses_diverged_history` and `a_branch_gone_from_the_origin_is_reported_not_fatal`.

#### Scenario: Rewritten upstream
- **WHEN** the origin's branch was force-pushed so the installed commit is not an ancestor of its HEAD
- **THEN** `update ID` (and `update ID --dev`) fails with `version_conflict` whose message names ID and whose structured error carries ID, and the workspace is unchanged, while `update ID --ref BRANCH` moves to the new HEAD

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has any lost-work item (install-uninstall), staged files included. The complete library message SHALL name the package and describe the lost-work items; FFI SHALL preserve its ID and items, and CLI SHALL print that message unchanged. Both SHALL count a tag the origin had with the same object at the last fetch as published, not `UnpublishedTag`; rollback stays offline, while `update` also accepts a tag the origin has now.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the clone's HEAD was moved away from the installed commit and the user runs `rollback ID`
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

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` offline (no fetch) and redeploy; without a previous revision, or when its commit is no longer in the clone, it SHALL fail with `no_previous_version`. It SHALL print `ID rolled back A → B`. Like update, it is refused with `dependency_conflict` when the target version breaks an installed dependent (see dependencies, "Updates keep dependents working"). Verified by `rollback_to_a_pruned_commit_names_it`.

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

#### Scenario: Previous commit gone
- **WHEN** the previous revision's commit is no longer in the clone (pruned after the origin deleted its tag)
- **THEN** rollback fails with `no_previous_version` ("previous revision COMMIT of ID is no longer available") and the workspace is unchanged

### Requirement: Joint upgrade of all packages
`update all` (`Manager::update_all`) SHALL resolve all installed packages together with PubGrub over the candidates of each package's mode ("Pre-releases are opt-in"), the installed revision as fallback and missing dependencies from the configured catalog; preferring newer versions, installed packages first in ID order, applied as one recoverable transaction. Each entry carries `major`. Verified by `update_all_coordinates_versions_and_adds_dependencies_atomically`.

#### Scenario: App and dependency cross a major boundary together
- **WHEN** app 2.0 requires lib ^2 and lib 2.0 exists, while app 1.x is installed with lib 1.x
- **THEN** `update all` moves both in one transaction and marks both lines `(major)`

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

### Requirement: update all dry run
`update all --dry-run` (`update_all_plan`) SHALL resolve and stage payloads to validate them but SHALL NOT commit; output SHALL use `would be updated` / `would be installed` and end with `Nothing was changed.` Apply SHALL resolve again under the lock, so a preview is not a saved plan.

#### Scenario: Preview
- **WHEN** the user runs `gitpm update all --dry-run`
- **THEN** each package gets one line, the totals line ends with `Nothing was changed.` and `state.json` is unchanged

### Requirement: update all reports every package
`UpdateAllPlan` (of `update_all_plan` and `update_all`) SHALL list every installed package in `updates`, `kept_back` or `unchanged` (with their version: packages that stay at their installed revision with no newer release, or with a `reason`); a package updated to a release older than its latest is in both `updates` and `kept_back`, and `installs` lists only new dependencies. The output is built from the plan alone. Verified by `update_all_plan_lists_unchanged_packages`.

#### Scenario: One update, one current
- **WHEN** `a` has a newer release and `b` is current, and the user runs `update all`
- **THEN** the plan has `a` in `updates` and `b` with `2.0.0` in `unchanged`, and the CLI prints `b up to date (2.0.0)` from it
- **AND** a broken consumer is in `unchanged` with its reason also when it has a newer release

### Requirement: update follows the recorded mode
`update ID` without a target SHALL follow the package's recorded mode: release mode moves to the newest release, branch mode to the branch HEAD, and a pinned package SHALL stay unchanged with reason `pinned` (CLI: `ID pinned at V (REF)`). `--ref REF` and `--dev` SHALL move the package and record the new mode as at install (install-uninstall); `--release` SHALL return it to release mode at the newest release. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `acme/greeter` was installed with `--ref acme/greeter@0.1.0`, 0.2.0 is released and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)` and nothing changes

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
- **THEN** the installed commit is kept, the mode becomes release (a change), and the CLI warns `release tag ID@0.1.0 now points to COMMIT; the installed commit is kept`; this is covered by `return_to_releases_at_the_installed_version_keeps_the_commit`

### Requirement: A change of mode alone is a change
An update whose target resolves to the installed commit but records another mode SHALL write the new mode and report a change (`RevisionChange.changed`, `from` and `to` at the same commit; CLI `ID pinned at V (REF)`, `ID follows branch B at V`, `ID follows releases at V`). Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pin at the installed commit
- **WHEN** `acme/greeter` follows releases at 0.2.0 and the user runs `update acme/greeter --ref acme/greeter@0.2.0`
- **THEN** it prints `acme/greeter pinned at 0.2.0 (acme/greeter@0.2.0)` and the record says `{"pinned": "acme/greeter@0.2.0"}`

### Requirement: Rollback restores revision and mode together
`rollback ID` SHALL swap `current` with `previous` and the recorded mode with the mode `previous` carries (`previous.mode`, written by every update), so a rollback after `update ID --release` of a branch-mode package follows the branch again. This adds to "Rollback swaps current and previous". Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Branch, release, rollback
- **WHEN** a package follows `main`, the user runs `update ID --release` and then `rollback ID`
- **THEN** it is back at the branch commit in branch mode `main`, and `previous` is the release with no `mode`

### Requirement: Protected packages update through their catalog
A protected package (recorded `catalog`) SHALL update only to versions its verified catalog lists, read from the recorded location whatever `Options.catalog` is; the release commit, package folder, manifest and file hashes SHALL match the record before any change (`catalog` otherwise). `update ID --detach` (`Manager::update_with`) SHALL drop the protection and update from the origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `acme/greeter` 0.2.0 has the tag `acme/greeter@0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update acme/greeter` prints `acme/greeter kept at 0.2.0 (upstream 0.3.0 not in catalog)` and `update all` lists it in `unchanged` with that reason
- **AND** a ref or branch target needs a detach: `update acme/greeter --ref acme/greeter@0.3.0` fails with `invalid_argument` ("… detach it to follow a ref or branch of its origin")

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

### Requirement: Major updates are marked
An update to a newer version outside the caret range of the installed one (a new MAJOR; for 0.x a new MINOR; for 0.0.x any newer version) SHALL be marked `major` in its `RevisionChange` and in each `update all` entry, and the CLI SHALL append ` (major)` to its line. Authors mark a breaking release only by its version number. A major update is not held back by itself; dependents stay protected by their own requirements. Verified by `workspace::major_updates_are_marked`.

#### Scenario: Major line
- **WHEN** `acme/greeter` 1.2.3 is installed, 2.0.0 is released and nothing depends on it, and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter updated 1.2.3 → 2.0.0 (major)` and the `RevisionChange` has `major` true

#### Scenario: 0.x minor
- **WHEN** a package moves from 0.2.5 to 0.3.0
- **THEN** the change is marked `(major)`; a move from 0.2.5 to 0.2.6 is not, nor one from `2.0.0-beta.1` to `2.0.0`

#### Scenario: A dependent caps the update
- **WHEN** `acme/app` requires `acme/greeter ^1.2.0` and the user runs `update acme/greeter` with 2.0.0 released
- **THEN** it fails with `dependency_conflict` and the workspace is unchanged (`cycles_resolve_and_updates_keep_dependents_working`)

### Requirement: Pre-releases are opt-in
Install, update and `update all` SHALL never pick a pre-release implicitly. A pre-release SHALL be installed only when named (`--ref ID@1.3.0-beta.1`, `install-catalog ID --version 1.3.0-beta.1`, a dependency range naming it). Once one is installed in release mode, `update` and `update all` SHALL also consider newer pre-releases of the same MAJOR.MINOR.PATCH besides every newer release. A pre-release tag not considered never blocks an update. Verified by `workspace::pre_releases_are_opt_in`.

#### Scenario: Newest tag is a pre-release
- **WHEN** a package is at 1.2.0 and the origin also has `1.3.0-beta.1`
- **THEN** `update` reports `up to date (1.2.0)`

#### Scenario: A ref install is pinned
- **WHEN** the user installs with `--ref ID@1.3.0-beta.1` and later runs `update ID --release`
- **THEN** the package stays pinned until then; in release mode it follows the 1.3.0 track

#### Scenario: Only pre-release tags
- **WHEN** a repository has only the tag `ID@1.0.0-beta.1`
- **THEN** `install` fails with `no_releases` and `install SOURCE --ref ID@1.0.0-beta.1` installs it

#### Scenario: A consumer's range names a pre-release
- **WHEN** a consumer requires `lib ^1.3.0-beta.1`
- **THEN** a fresh install of `lib` for it takes `1.3.0-beta.1` (`dependency_ranges_pick_pre_releases_only_when_named`); `update all` never moves an installed release of `lib` onto a pre-release

#### Scenario: Following a beta
- **WHEN** `1.3.0-beta.1` is installed with `install-catalog ID --version 1.3.0-beta.1` and the origin has `1.3.0-beta.2` and `1.4.0-beta.1`, later also `1.3.0`
- **THEN** `update` moves to `1.3.0-beta.2`, then to `1.3.0`, and never to `1.4.0-beta.1`
