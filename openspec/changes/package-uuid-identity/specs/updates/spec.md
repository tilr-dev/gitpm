## MODIFIED Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the origin and move to the newest release tag, across major versions and passing over pre-releases ("Major updates are marked", "Pre-releases are opt-in"). When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (same version, another commit: `ID updated V (old → new)`). Failures change nothing (errors). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced` twice
- **THEN** the first prints `d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)`, the second `d135a7cf-1c63-549a-93cb-8f89656b2ced up to date (0.2.0)`

### Requirement: Updates need no catalog
`update ID` (any mode) and `update all` SHALL read releases and branches from the clone origin (set from `packages.<id>.source`) without catalog reads for unprotected Git packages. `check` SHALL use that origin when the catalog does not list them. Builds SHALL instead follow their recorded catalog, including unverified catalogs, without a catalog option. Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced` prints `d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced would be updated 0.1.0 → 0.2.0 (major)` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `d135a7cf-1c63-549a-93cb-8f89656b2ced` says `update available` with `0.2.0 (major)` from the origin, and nothing in the workspace changes

#### Scenario: Unverified build follows its recorded catalog
- **WHEN** a build from an unverified catalog is updated without a catalog option
- **THEN** it reads its recorded catalog and installs the newer archive without cloning

#### Scenario: Select the target source kind
- **WHEN** update selects a release with a build, or without one
- **THEN** it installs the former without Git and the latter through Git; explicit ref, branch, dev or detach transitions a build to Git, and new dependencies follow the dependency-resolution rules

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse lost work in the affected worktree, including staged files, with `local_changes`. The library message SHALL name the package and items; FFI SHALL preserve them and CLI print the message unchanged. Tags with the origin's object at the last fetch count as published; update also accepts currently published tags. Rollback stays offline.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the package worktree's HEAD was moved away from the installed commit and the user runs `rollback ID`
- **THEN** rollback fails with `local_changes` listing `HeadMoved`
- Verified by `local_work_blocks_update_and_rollback`, `update_and_rollback_refuse_hidden_local_work` and `rollback_of_a_fork_treats_fetched_upstream_tags_as_published`.

#### Scenario: Fork, update, rollback
- **WHEN** `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4`, a fork whose repository also holds its upstream's tags (for example `v1.0.0` on a commit naming `7e6e8392-7be8-55ae-bd03-2b430b1c5d59`), is installed at 1.0.0, updated to 1.1.0, and the user runs `rollback ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4`
- **THEN** the upstream tags fetched from the fork's origin count as published, as they did for the update, and rollback prints `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 rolled back 1.1.0 → 1.0.0` without a network access

#### Scenario: Retained upstream tag on rollback
- **WHEN** the clone of a fork keeps a `vVERSION` tag of its upstream that no remote branch contains, and the fork's origin had that tag with the same object at the last fetch, and the user runs `rollback`
- **THEN** rollback works offline and counts the tag as published, as `update` does, so it does not fail with `local_changes` because of it

#### Scenario: Tag created locally
- **WHEN** a tag was created in the clone by hand and no fetch from the origin brought it
- **THEN** update and rollback alike fail with `local_changes` listing `UnpublishedTag`

#### Scenario: Stash blocks update
- **WHEN** the clone has a stash entry and the user runs `update ID`
- **THEN** update fails with `local_changes` and nothing changes

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` revisions and modes offline (no fetch) and redeploy retained Git or build output. Missing previous revisions, Git commits or cached builds SHALL fail with `no_previous_version`. It SHALL print `ID rolled back A → B` and refuse a target that breaks an installed dependent with `dependency_conflict` (dependencies, "Updates keep dependents working"). Verified by `rollback_to_a_pruned_commit_names_it`.

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

#### Scenario: Previous commit gone
- **WHEN** the previous revision's commit is no longer in the clone (pruned after the origin deleted its tag)
- **THEN** rollback fails with `no_previous_version` ("previous revision COMMIT of ID is no longer available") and the workspace is unchanged

#### Scenario: Offline build rollback
- **WHEN** a build was updated and its catalog and artifact server are unavailable
- **THEN** rollback restores the retained previous output and metadata and swaps the two revisions

#### Scenario: Restore retained build state
- **WHEN** rollback selects a retained build revision
- **THEN** its output, release metadata and catalog provenance are restored locally, and current and previous modes and source kinds are swapped

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
- **WHEN** a clone of `d135a7cf-1c63-549a-93cb-8f89656b2ced` has an unpublished local tag and `update all` is run
- **THEN** it fails with `local_changes`, whose library message names `d135a7cf-1c63-549a-93cb-8f89656b2ced` and the tag, and CLI prints that message unchanged
- **AND** FFI preserves `id` `d135a7cf-1c63-549a-93cb-8f89656b2ced` and the lost-work items, and the workspace stays unchanged

### Requirement: update follows the recorded mode
Managed `update ID` SHALL follow recorded mode: release moves to the newest release, branch to HEAD, pinned stays with reason `pinned` (CLI: `ID pinned at V (REF)`). Explicit targets SHALL switch managed mode as at install. Links follow "Linked packages report releases without automatic replacement", including explicit targets. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` was installed with `--ref v0.1.0`, 0.2.0 is released and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced pinned at 0.1.0 (v0.1.0)` and nothing changes

#### Scenario: Back to releases
- **WHEN** the user then runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced --release`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)` and the record has no `mode`

#### Scenario: Explicit managed targets
- **WHEN** a managed package is updated with `--ref REF`, `--dev` or `--release`
- **THEN** the target and recorded mode follow install semantics: explicit ref, origin default branch or newest release respectively

### Requirement: A change of mode alone is a change
An update whose target resolves to the installed commit but records another mode SHALL write the new mode and report a change (`RevisionChange.changed`, `from` and `to` at the same commit; CLI `ID pinned at V (REF)`, `ID follows branch B at V`, `ID follows releases at V`). Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pin at the installed commit
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` follows releases at 0.2.0 and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced --ref v0.2.0`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced pinned at 0.2.0 (v0.2.0)` and the record says `{"pinned": "v0.2.0"}`

### Requirement: Protected packages update through their catalog
Protected packages SHALL update only to versions in their recorded verified catalog, whatever `Options.catalog` is. Git targets SHALL match recorded source, commit, path, manifest and hashes before any change (`catalog` otherwise); builds SHALL verify archive and files without Git. `update ID --detach` (`Manager::update_with`) SHALL drop protection and update from origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.2.0 has the tag `v0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update d135a7cf-1c63-549a-93cb-8f89656b2ced` prints `d135a7cf-1c63-549a-93cb-8f89656b2ced kept at 0.2.0 (upstream 0.3.0 not in catalog)` and `update all` lists it in `unchanged` with that reason
- **AND** a ref or branch target needs a detach: `update d135a7cf-1c63-549a-93cb-8f89656b2ced --ref v0.3.0` fails with `invalid_argument` ("… detach it to follow a ref or branch of its origin")

#### Scenario: Hash mismatch on update
- **WHEN** the verified catalog lists 0.3.0 but the files at the recorded commit do not match its hashes
- **THEN** `update d135a7cf-1c63-549a-93cb-8f89656b2ced` and `update all` fail with `catalog` ("catalog file hashes do not match git dist of d135a7cf-1c63-549a-93cb-8f89656b2ced@0.3.0: the release differs from the one added to the catalog") and the workspace is unchanged; a release commit the origin does not have fails with `catalog` ("… is not on the origin")

#### Scenario: Catalog no longer verified or unreachable
- **WHEN** the owner removed `"verified": true` from the catalog that protects `d135a7cf-1c63-549a-93cb-8f89656b2ced`, or the catalog cannot be read
- **THEN** `update d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `catalog` ("… is no longer verified; detach …") or `http` and nothing changes
- **AND** `update all` does not fail: it keeps `d135a7cf-1c63-549a-93cb-8f89656b2ced` in `unchanged` with the reason `catalog_unavailable` (`d135a7cf-1c63-549a-93cb-8f89656b2ced kept at 0.2.0 (catalog unavailable: REASON)`) and updates the other packages
- **AND** this is covered by `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`

#### Scenario: New dependency of a protected package
- **WHEN** protected `b879842f-16c2-558f-9d03-a271dc901f57` 1.1.0 in its verified catalog adds a dependency `cfbc4d17-8707-5571-853d-c9136a7f9f21` that is not installed, and no catalog or an unverified one is configured
- **THEN** `update b879842f-16c2-558f-9d03-a271dc901f57` and `update all` install `cfbc4d17-8707-5571-853d-c9136a7f9f21` from the recorded verified catalog and record it protected by that catalog
- **AND** this is covered by `workspace::new_dependencies_of_a_protected_package_come_from_its_catalog`

#### Scenario: Detach
- **WHEN** the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced --detach`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced detached from catalog LOCATION; it now follows its origin` and `d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.2.0 → 0.3.0 (major)`, the record loses `catalog` and later updates follow the origin
- **AND** a detach with no newer release is still a change (`workspace::detach_without_a_new_release_is_a_change`); detaching an unprotected package fails with `invalid_argument`; `update all --detach` is a usage error (exit status 1)

#### Scenario: Detach a protected build
- **WHEN** a protected build is updated with detach or an explicit Git ref or dev target
- **THEN** the transition stages a Git clone, drops protection and deploys Git output atomically

#### Scenario: Build provenance and protection
- **WHEN** an unverified catalog supplies a build
- **THEN** recorded build provenance does not imply protection; an explicit ref, branch or dev target transitions a build to Git atomically, while protected Git packages require detach for refs

### Requirement: Major updates are marked
An update to a newer version outside the caret range of the installed one (a new MAJOR; for 0.x a new MINOR; for 0.0.x any newer version) SHALL be marked `major` in its `RevisionChange` and in each `update all` entry, and the CLI SHALL append ` (major)` to its line. Authors mark a breaking release only by its version number. A major update is not held back by itself; dependents stay protected by their own requirements. Verified by `workspace::major_updates_are_marked`.

#### Scenario: Major line
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 1.2.3 is installed, 2.0.0 is released and nothing depends on it, and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced updated 1.2.3 → 2.0.0 (major)` and the `RevisionChange` has `major` true

#### Scenario: 0.x minor
- **WHEN** a package moves from 0.2.5 to 0.3.0
- **THEN** the change is marked `(major)`; a move from 0.2.5 to 0.2.6 is not, nor one from `2.0.0-beta.1` to `2.0.0`

#### Scenario: A dependent caps the update
- **WHEN** `b879842f-16c2-558f-9d03-a271dc901f57` requires `d135a7cf-1c63-549a-93cb-8f89656b2ced ^1.2.0` and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced` with 2.0.0 released
- **THEN** it fails with `dependency_conflict` and the workspace is unchanged (`cycles_resolve_and_updates_keep_dependents_working`)
