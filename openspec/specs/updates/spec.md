# updates Specification

## Purpose

Defines how installed packages move between revisions: single-package `update` (stable, `--ref`, dev channel), `rollback`, the joint `update all` upgrade, and the rules that protect local work and published releases.

## Requirements

### Requirement: Stable update moves to the newest release
`update ID` SHALL fetch the package's origin and move to the numerically newest release tag. When already there it SHALL report `ID up to date (V)` and change nothing; otherwise `ID updated A → B` (same version on another commit: `ID updated V (old → new)` with short commits). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy`.

#### Scenario: Update and repeat
- **WHEN** 0.2.0 is released after installing 0.1.0 and the user runs `update acme/greeter` twice
- **THEN** the first prints `acme/greeter updated 0.1.0 → 0.2.0`, the second `acme/greeter up to date (0.2.0)`

### Requirement: Stable update never downgrades or accepts mutated releases
A stable update SHALL refuse with `version_conflict` when the newest release is older than the installed one ("downgrading requires an explicit ref"), when the installed version's release commit changed, when an observed release tag moved to another commit, or when upstream changed without a version bump. Verified by `stable_update_never_downgrades`, `stable_update_refuses_a_release_whose_commit_changed`, `release_manifest_must_match_tag_and_update_rejects_retagging`.

#### Scenario: Retagged release
- **WHEN** the tag of an already seen release `acme/greeter@0.2.0` is moved to another commit upstream
- **THEN** update fails with `version_conflict` ("release tag acme/greeter@0.2.0 changed commit") and the workspace is unchanged

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit upstream
- **THEN** update fails with `version_conflict` ("release commit changed for installed version", without the tag name)

### Requirement: Update mirrors upstream tags
A single-package update SHALL mirror upstream tags (pruned and force-fetched): a release deleted upstream disappears from the clone and a moved one is seen at its new commit and refused as above. Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** upstream deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and update fails with `version_conflict` ("latest release is older than installed version; downgrading requires an explicit ref")

### Requirement: Explicit ref resolves against upstream
`update ID --ref REF` SHALL resolve REF in order: tag, fetched upstream branch `refs/remotes/origin/REF`, commit ID; `HEAD` SHALL mean upstream HEAD (after `git remote set-head origin --auto`). Local branches of the installed clone (`refs/heads/*`) SHALL be refused as stale (`invalid_argument`). An explicit ref MAY move to an older version but MUST change the version. `--ref` with `--channel dev` is refused (`invalid_argument`). Verified by `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** it fails with `version_conflict` ("upstream changed without a version bump")

#### Scenario: Branch name
- **WHEN** upstream branch `feature` moved after install and the user runs `update ID --ref feature`
- **THEN** the package moves to the fetched `origin/feature` commit, not the clone's stale local branch

### Requirement: Dev channel follows upstream HEAD by commit
`update ID --channel dev` SHALL move to upstream HEAD of the default branch; a new commit SHALL count as an update even without a version bump. It SHALL refuse with `version_conflict` when upstream history diverged from the installed commit ("upstream history diverged; automatic dev update refused"). Verified by `dev_channel_follows_origin_head_and_refuses_diverged_history`.

#### Scenario: Rewritten upstream
- **WHEN** upstream force-pushed so the installed commit is not an ancestor of HEAD
- **THEN** `update --channel dev` fails with `version_conflict`

### Requirement: Local work blocks update and rollback
Update and rollback SHALL refuse with `local_changes` when the clone has modified, untracked, ignored or staged files, a HEAD moved from the installed commit (`HeadMoved`), local commits, or unpublished local branches, tags or stash entries. Only `update`, which fetches anyway, SHALL treat a tag that exists on the origin with the same object as published. Verified by `local_work_blocks_update_and_rollback` and `update_and_rollback_refuse_hidden_local_work`.

#### Scenario: Retained upstream tag on rollback
- **WHEN** a fork's clone keeps an upstream tag that no remote branch contains and the user runs `rollback`
- **THEN** rollback works offline and counts the tag as `UnpublishedTag`, so it fails with `local_changes`, while `update` accepts it because the origin has it

#### Scenario: Stash blocks update
- **WHEN** the clone has a stash entry and the user runs `update ID`
- **THEN** update fails with `local_changes` and nothing changes

### Requirement: Failed updates change nothing
A failed update or rollback SHALL leave files and state byte-identical (general rule in errors, "Failing operations leave the workspace unchanged"). Verified by `failed_update_keeps_files_and_state` and `workspace::update_refusals_leave_the_workspace_unchanged`.

#### Scenario: Network failure mid-update
- **WHEN** the fetch fails
- **THEN** the command fails with `git` and the workspace tree hash is unchanged

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` offline and redeploy; without a previous revision it SHALL fail with `no_previous_version`. It SHALL print `ID rolled back A → B`. Like update, it is refused with `dependency_conflict` when the target version breaks an installed dependent (see dependencies, "Updates keep dependents working").

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

### Requirement: Joint upgrade of all packages
`update all` (`Manager::update_all`) SHALL resolve all installed packages together with PubGrub over stable release tags of each origin, the installed revision as fallback and missing dependencies from the configured catalog; it SHALL prefer newer versions with installed packages prioritized in lexical ID order, and apply the whole result as one recoverable transaction. Verified by `update_all_coordinates_versions_and_adds_dependencies_atomically`.

#### Scenario: App and dependency cross a major boundary together
- **WHEN** app 2.0 requires lib ^2 and lib 2.0 exists, while app 1.x is installed with lib 1.x
- **THEN** `update all` moves both in one transaction

### Requirement: update all never removes or downgrades
`update all` SHALL NOT remove or downgrade packages, SHALL preserve replacement bindings and disabled flags (disabled packages cannot satisfy dependencies), SHALL report packages kept back with their latest version, and SHALL fail with `dependency_conflict` carrying the solver's explanation when no compatible solution repairs a broken state. Verified by `update_all_does_not_downgrade_or_accept_mutated_releases` and `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`.

#### Scenario: Kept back
- **WHEN** the newest release of a package does not fit a dependent's requirement
- **THEN** the CLI prints `ID kept back at V (latest L: dependency constraints)`, or, when it advances to an intermediate compatible release, `ID updated A → B (latest L kept back by dependency constraints)`

### Requirement: update all is stable-only and refuses local work
`update all` SHALL support stable releases only (`--ref` and `--channel dev` are refused) and SHALL refuse local changes, unpublished work and moved release tags in any clone without partially deploying. Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_previews_applies_and_rejects_unsupported_options`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all supports stable releases only; --ref and --channel dev are not supported`

### Requirement: update all dry run
`update all --dry-run` (`update_all_plan`) SHALL resolve and stage payloads to validate them but SHALL NOT commit; output SHALL use `would be updated` / `would be installed` and end with `Nothing was changed.` Apply SHALL resolve again under the lock, so a preview is not a saved plan.

#### Scenario: Preview
- **WHEN** the user runs `gitpm update all --dry-run`
- **THEN** each package gets one line, the totals line ends with `Nothing was changed.` and `state.json` is unchanged
