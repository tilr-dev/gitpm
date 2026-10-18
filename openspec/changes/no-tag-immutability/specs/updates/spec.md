## RENAMED Requirements

- FROM: `### Requirement: Stable update never downgrades or accepts mutated releases`
- TO: `### Requirement: Stable update never downgrades`

## MODIFIED Requirements

### Requirement: Stable update never downgrades
A stable update SHALL compare versions only and SHALL never downgrade. When the installed release was retracted (tag deleted on the origin) and only older ones remain, the package SHALL stay and be reported up to date with the warning `installed release ID@V no longer exists on the origin`; when the tag of the installed version moved, it SHALL stay up to date with the warning `release tag ID@V now points to COMMIT; the installed commit is kept`. Verified by `stable_update_never_downgrades`.

#### Scenario: Release commit of the installed version moved
- **WHEN** the tag of the installed version is moved to another commit on the origin and the user runs `update ID`
- **THEN** it prints `ID up to date (V)` and the moved-tag warning, and the workspace is unchanged

#### Scenario: Retracted release in update all
- **WHEN** the origin deleted the tag of the installed release and the user runs `update all`
- **THEN** the package is listed in `unchanged` with the retraction warning, as for `update ID`

### Requirement: Update mirrors the origin's tags
A single-package update of a package with its own clone SHALL mirror the origin's tags (pruned and force-fetched): a release deleted on the origin disappears from the clone and a moved one is seen at its new commit (package-ids). When the deleted tag is the installed (newest) release, `update ID` and `update all` report the package up to date with a warning ("Stable update never downgrades"). Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` prints `ID up to date (V)` with `Warning: installed release ID@V no longer exists on the origin`, changing nothing

### Requirement: update all honors each package's mode and refuses local work
`update all` SHALL take no target: it SHALL solve release-mode packages over their release tags, offer each branch-mode package its branch HEAD as only candidate, list pinned packages in `unchanged` with reason `pinned`, and refuse lost work (`local_changes`) in any clone without partially deploying; moved release tags do not fail it (package-ids). Verified by `update_all_preserves_local_work_and_never_partially_deploys` and `update_all_follows_each_mode`.

#### Scenario: update all with --ref
- **WHEN** the user runs `gitpm update all --ref x`
- **THEN** it fails with `update all takes no --ref, --dev or --release; it follows each package's recorded mode` and changes nothing

#### Scenario: Mixed modes
- **WHEN** one package is in release mode with a newer release, one in branch mode whose branch moved, and one pinned
- **THEN** `update all` updates the first two and prints `C pinned at V (REF)` for the third

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` offline (no fetch) and redeploy; without a previous revision it SHALL fail with `no_previous_version`, also when the previous commit is no longer in the clone (pruned after the origin deleted or moved its tag), naming ID and the commit. It SHALL print `ID rolled back A → B`. Like update, it is refused with `dependency_conflict` when the target version breaks an installed dependent (dependencies, "Updates keep dependents working").

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

#### Scenario: Previous commit gone
- **WHEN** the previous revision's commit was pruned from the own clone after the origin deleted its tag
- **THEN** rollback fails with `no_previous_version` ("previous revision COMMIT of ID is no longer available") and the workspace is unchanged
