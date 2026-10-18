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

## MODIFIED Requirements

### Requirement: Branch mode follows the branch HEAD by commit
`update ID` of a package in branch mode SHALL move to the HEAD of its recorded branch on the origin; a new commit SHALL count as an update even without a version bump. It SHALL refuse with `version_conflict` when the origin's history diverged from the installed commit ("history of BRANCH diverged; automatic update refused"). Verified by `branch_mode_follows_origin_head_and_refuses_diverged_history`.

#### Scenario: Rewritten upstream
- **WHEN** the origin's branch was force-pushed so the installed commit is not an ancestor of its HEAD
- **THEN** `update ID` fails with `version_conflict` and the workspace is unchanged

### Requirement: update all honors each package's mode and refuses local work
`update all` SHALL take no target: it SHALL solve release-mode packages over their release tags, offer each branch-mode package its branch HEAD as only candidate, list pinned packages in `unchanged` with reason `pinned`, and refuse lost work (`local_changes`) and moved release tags (package-ids, "Moved release tags are refused") in any clone without partially deploying. Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_follows_each_mode`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all takes no --ref, --dev or --release; it follows each package's recorded mode` and changes nothing

#### Scenario: Mixed modes
- **WHEN** one package is in release mode with a newer release, one in branch mode whose branch moved, and one pinned
- **THEN** `update all` updates the first two and prints `C pinned at V (REF)` for the third

### Requirement: Explicit ref resolves against the origin
`update ID --ref REF` SHALL resolve REF in order: tag, fetched origin branch `refs/remotes/origin/REF`, commit ID; `HEAD` SHALL mean the origin's HEAD (after `git remote set-head origin --auto`). Local branches of the installed clone (`refs/heads/*`) SHALL be refused as stale (`invalid_argument`). A tag or commit MAY move to an older version but MUST change the version; a branch records branch mode. Verified by `update_to_a_branch_uses_the_fetched_upstream_branch`.

#### Scenario: Same version on another commit
- **WHEN** `update ID --ref COMMIT` names a commit whose manifest has the installed version
- **THEN** it fails with `version_conflict` ("… changed without a version bump")

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
