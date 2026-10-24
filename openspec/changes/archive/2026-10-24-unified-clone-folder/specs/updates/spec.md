## MODIFIED Requirements

### Requirement: Update mirrors the origin's tags
An update of a Git package SHALL mirror the origin's tags (pruned and force-fetched): a deleted release disappears from the clone, a moved one is seen at its new commit (package-ids). When the deleted tag is the installed (newest) release, `update ID` and `update all` report the package up to date with a warning ("Stable update never downgrades"). Verified by `stable_update_follows_deleted_and_moved_upstream_tags`.

#### Scenario: Deleted installed release tag
- **WHEN** the origin deletes the release tag of the installed (newest) version
- **THEN** the tag disappears from the clone and `update ID` prints `ID up to date (V)` with `Warning: installed release ID@V no longer exists on the origin`, changing nothing
