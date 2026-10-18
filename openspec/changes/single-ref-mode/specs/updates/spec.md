## RENAMED Requirements

- FROM: `### Requirement: Dev channel follows the origin HEAD by commit`
- TO: `### Requirement: Branch mode follows the branch HEAD by commit`
- FROM: `### Requirement: update all is stable-only and refuses local work`
- TO: `### Requirement: update all honors each package's mode and refuses local work`

## ADDED Requirements

### Requirement: update follows the recorded mode
`update ID` without a target SHALL follow the package's recorded mode: release mode moves to the newest release, branch mode to the branch HEAD, and a pinned package SHALL stay unchanged with reason `pinned` (CLI: `ID pinned at V (REF)`). `--ref REF` and `--dev` SHALL move the package and record the new mode as at install (install-uninstall); `--release` SHALL return it to release mode at the newest release. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `acme/greeter` was installed with `--ref acme/greeter@0.1.0`, 0.2.0 is released and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)` and nothing changes

#### Scenario: Back to releases
- **WHEN** the user then runs `update acme/greeter --release`
- **THEN** it prints `acme/greeter updated 0.1.0 → 0.2.0` and the record has no `mode`

### Requirement: An explicit return to releases may go down
`update ID --release` SHALL move to the newest release also when it is below the installed version; following releases (`update ID` in release mode) never goes down. Verified by `workspace::stable_update_never_downgrades`.

#### Scenario: Back to releases from above
- **WHEN** a package follows `main` at 0.4.0, the newest release is 0.3.0 and the user runs `update ID --release`
- **THEN** it moves down to 0.3.0 in release mode; a plain `update` of a release-mode package whose newest release is below the installed version fails with `version_conflict` ("latest release is older …")

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

## MODIFIED Requirements

### Requirement: Branch mode follows the branch HEAD by commit
`update ID` of a package in branch mode SHALL move to the HEAD of its recorded branch on the origin; a new commit SHALL count as an update even without a version bump. It SHALL refuse with `version_conflict` when the origin's history diverged from the installed commit ("history of BRANCH diverged; automatic update refused") and when the origin no longer has the branch ("the origin no longer has the branch B that ID follows; automatic update refused"). An explicit `--ref BRANCH` SHALL skip the ancestor check: it is the way to follow a branch again after a force-push. Verified by `branch_mode_follows_origin_head_and_refuses_diverged_history` and `a_branch_gone_from_the_origin_is_reported_not_fatal`.

#### Scenario: Rewritten upstream
- **WHEN** the origin's branch was force-pushed so the installed commit is not an ancestor of its HEAD
- **THEN** `update ID` (and `update ID --dev`) fails with `version_conflict` and the workspace is unchanged, while `update ID --ref BRANCH` moves to the new HEAD

### Requirement: update all honors each package's mode and refuses local work
`update all` SHALL take no target: it SHALL solve release-mode packages over their release tags, offer each branch-mode package its branch HEAD as only candidate, list pinned packages in `unchanged` with reason `pinned` (CLI `ID pinned at V (REF)`), list a branch-mode package whose history diverged with reason `diverged` (`ID kept at V (history of B diverged)`) and one whose branch is gone from the origin with reason `branch_gone` (`ID kept at V (branch B is gone)`), and refuse lost work (`local_changes`) and moved release tags (package-ids, "Moved release tags are refused") in any clone without partially deploying. Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_follows_each_mode`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all takes no --ref, --dev or --release; it follows each package's recorded mode` and changes nothing

#### Scenario: Mixed modes
- **WHEN** one package is in release mode with a newer release, one in branch mode whose branch moved, and one pinned
- **THEN** `update all` updates the first two and prints `C pinned at V (REF)` for the third

#### Scenario: Branch gone or diverged
- **WHEN** the branch a package follows was deleted on the origin, or force-pushed
- **THEN** `update all` updates the other packages and lists it in `unchanged` with reason `branch_gone` or `diverged`

### Requirement: Explicit ref resolves against the origin
`update ID --ref REF` (and `install --ref REF`) SHALL resolve REF in order, by exact name only: tag `refs/tags/REF`, fetched origin branch `refs/remotes/origin/REF`, commit ID (7 to 64 lowercase hex digits); `HEAD` and `refs/remotes/origin/HEAD` SHALL mean the origin's default branch (after `git remote set-head origin --auto`), recorded by its name. Revision expressions (`HEAD~1`, `TAG^`) and local branches of the installed clone (`refs/heads/*`, stale) SHALL be refused with `invalid_argument`. A tag or commit MAY move to an older version but MUST change the version, except when the package leaves branch mode (its branch commit may carry the version of a release); a branch records branch mode. Verified by `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** it fails with `version_conflict` ("… changed without a version bump"), unless the package was in branch mode

#### Scenario: Revision expression
- **WHEN** the user runs `install SOURCE --ref HEAD~1` or `update ID --ref main~1`
- **THEN** it fails with `invalid_argument` ("unknown ref …: not a tag, origin branch or commit") and nothing changes

#### Scenario: Branch name
- **WHEN** the origin's branch `feature` moved after install and the user runs `update ID --ref feature`
- **THEN** the package moves to the fetched `origin/feature` commit, not the clone's stale local branch, and is recorded in branch mode `feature`

### Requirement: Updates need no catalog
`update ID` (in any mode) and `update all` SHALL read releases and branches from the origin of the package's clone (set from `packages.<id>.source` at install) and SHALL NOT read a catalog for installed packages, so they work without one; `check` reads the same origin for packages the catalog does not list (catalog, "check compares installed packages with their catalog or origin"). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

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
