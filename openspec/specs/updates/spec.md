# updates Specification

## Purpose

Defines how installed packages move between revisions: single-package `update` (stable, `--ref`, dev channel), `rollback`, the joint `update all` upgrade, and the rules that protect local work and published releases.

## Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the package's origin and move to the numerically newest release tag. When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (same version on another commit: `ID updated V (old → new)` with short commits). A failing update or rollback leaves the workspace unchanged (errors, "Failing operations leave the workspace unchanged"). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update acme/greeter` twice
- **THEN** the first prints `acme/greeter updated 0.1.0 → 0.2.0`, the second `acme/greeter up to date (0.2.0)`

### Requirement: Updates need no catalog
`update ID` (stable, `--ref`, `--channel dev`) and `update all` SHALL read releases from the origin of the package's clone (set from `packages.<id>.source` at install) and SHALL NOT read a catalog for installed packages, so they work without one; `check` reads the same origin for packages the catalog does not list (catalog, "check compares installed packages with their catalog or origin"). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `acme/greeter` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update acme/greeter` prints `acme/greeter updated 0.1.0 → 0.2.0`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `acme/greeter would be updated 0.1.0 → 0.2.0` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `acme/greeter` says `update available` with `0.2.0` from the origin, and nothing in the workspace changes

### Requirement: Stable update never downgrades or accepts mutated releases
A stable update SHALL refuse with `version_conflict` when the newest release is older than the installed one ("downgrading requires an explicit ref"), when the installed version's release commit changed, when an observed release tag moved to another commit (rule in package-ids, "Moved release tags are refused"), or when the origin changed without a version bump. Verified by `stable_update_never_downgrades` and `stable_update_refuses_a_release_whose_commit_changed`.

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit on the origin
- **THEN** update fails with `version_conflict` ("release commit changed for installed version") and the workspace is unchanged

### Requirement: Update mirrors the origin's tags
A single-package update of a package with its own clone SHALL mirror the origin's tags (pruned and force-fetched): a release deleted on the origin disappears from the clone and a moved one is seen at its new commit and refused as in package-ids ("Moved release tags are refused"). When the deleted tag is the installed (newest) release, a single update fails as below while `update all` reports the package up to date. Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` fails with `version_conflict` ("… downgrading requires an explicit ref")

### Requirement: Explicit ref resolves against the origin
`update ID --ref REF` SHALL resolve REF in order: tag, fetched origin branch `refs/remotes/origin/REF`, commit ID; `HEAD` SHALL mean the origin's HEAD (after `git remote set-head origin --auto`). Local branches of the installed clone (`refs/heads/*`) SHALL be refused as stale (`invalid_argument`). An explicit ref MAY move to an older version but MUST change the version. A ref with the dev channel is refused (install-uninstall). Verified by `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** it fails with `version_conflict` ("… changed without a version bump")

#### Scenario: Branch name
- **WHEN** the origin's branch `feature` moved after install and the user runs `update ID --ref feature`
- **THEN** the package moves to the fetched `origin/feature` commit, not the clone's stale local branch

### Requirement: Dev channel follows the origin HEAD by commit
`update ID --channel dev` SHALL move to the origin's HEAD of the default branch; a new commit SHALL count as an update even without a version bump. It SHALL refuse with `version_conflict` when the origin's history diverged from the installed commit ("upstream history diverged; automatic dev update refused"). Verified by `dev_channel_follows_origin_head_and_refuses_diverged_history`.

#### Scenario: Rewritten upstream
- **WHEN** the origin was force-pushed so the installed commit is not an ancestor of its HEAD
- **THEN** `update --channel dev` fails with `version_conflict`

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has any lost-work item (list in install-uninstall, "Uninstall is planned and confirmed"), staged files included. Only `update`, which fetches anyway, SHALL treat a tag that exists on the origin with the same object as published. Verified by `local_work_blocks_update_and_rollback` and `update_and_rollback_refuse_hidden_local_work`.

#### Scenario: Moved HEAD blocks rollback
- **WHEN** the clone's HEAD was moved away from the installed commit and the user runs `rollback ID`
- **THEN** rollback fails with `local_changes` listing `HeadMoved`

#### Scenario: Retained upstream tag on rollback
- **WHEN** the clone of a fork keeps a tag of its upstream that no remote branch contains and the user runs `rollback`
- **THEN** rollback works offline and counts the tag as `UnpublishedTag`, so it fails with `local_changes`, while `update` accepts it because the origin has it

#### Scenario: Stash blocks update
- **WHEN** the clone has a stash entry and the user runs `update ID`
- **THEN** update fails with `local_changes` and nothing changes

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` offline (no fetch) and redeploy; without a previous revision it SHALL fail with `no_previous_version`. It SHALL print `ID rolled back A → B`. Like update, it is refused with `dependency_conflict` when the target version breaks an installed dependent (see dependencies, "Updates keep dependents working").

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

### Requirement: Joint upgrade of all packages
`update all` (`Manager::update_all`) SHALL resolve all installed packages together with PubGrub over stable release tags of each origin, the installed revision as fallback and missing dependencies from the configured catalog (installed and recorded as in dependencies); it SHALL prefer newer versions with installed packages prioritized in lexical ID order, and apply the whole result as one recoverable transaction. Verified by `update_all_coordinates_versions_and_adds_dependencies_atomically`.

#### Scenario: App and dependency cross a major boundary together
- **WHEN** app 2.0 requires lib ^2 and lib 2.0 exists, while app 1.x is installed with lib 1.x
- **THEN** `update all` moves both in one transaction

### Requirement: update all never removes or downgrades
`update all` SHALL NOT remove or downgrade packages, SHALL preserve replacement bindings and disabled flags (a disabled package satisfies no dependency here), SHALL report packages kept back with their latest version, and SHALL fail with `dependency_conflict` carrying the solver's explanation when no compatible solution repairs a broken state. Verified by `update_all_does_not_downgrade_or_accept_mutated_releases` and `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`.

#### Scenario: Kept back
- **WHEN** the newest release of a package does not fit a dependent's requirement
- **THEN** the CLI prints `ID kept back at V (latest L: dependency constraints)`, or, when it advances to an intermediate compatible release, `ID updated A → B (latest L kept back by dependency constraints)`
- **AND** a package whose newer release needs a dependency that is not installed, with no catalog configured, is kept back the same way

### Requirement: update all is stable-only and refuses local work
`update all` SHALL support stable releases only (`--ref` and `--channel dev` are refused) and SHALL refuse lost work (`local_changes`) and moved release tags (package-ids, "Moved release tags are refused") in any clone without partially deploying. Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_previews_applies_and_rejects_unsupported_options`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all supports stable releases only; --ref and --channel dev are not supported`

### Requirement: update all dry run
`update all --dry-run` (`update_all_plan`) SHALL resolve and stage payloads to validate them but SHALL NOT commit; output SHALL use `would be updated` / `would be installed` and end with `Nothing was changed.` Apply SHALL resolve again under the lock, so a preview is not a saved plan.

#### Scenario: Preview
- **WHEN** the user runs `gitpm update all --dry-run`
- **THEN** each package gets one line, the totals line ends with `Nothing was changed.` and `state.json` is unchanged

### Requirement: update all reports every package
`UpdateAllPlan` (of `update_all_plan` and `update_all`) SHALL list every installed package in `updates`, `kept_back` or `unchanged` (packages that stay at their installed revision with no newer release, with their version); a package updated to a release older than its latest is in both `updates` and `kept_back`, and `installs` lists only new dependencies. The output is built from the plan alone. Verified by `update_all_plan_lists_unchanged_packages`.

#### Scenario: One update, one current
- **WHEN** `a` has a newer release and `b` is current, and the user runs `update all`
- **THEN** the plan has `a` in `updates` and `b` with `2.0.0` in `unchanged`, and the CLI prints `b up to date (2.0.0)` from it
