## ADDED Requirements

### Requirement: Tags are not policed; integrity via protected catalog
Single source for moved and deleted release tags: fetching into any clone (own or shared) SHALL take the origin's release tags as they are, so a moved tag is seen at its new commit and a deleted one disappears, and no operation SHALL fail because a tag moved. Installed revisions are kept by commit. Integrity of published versions is the job of a verified catalog (catalog). Verified by `releases::moved_and_deleted_tags_are_taken_as_they_are`.

#### Scenario: Retagged release
- **WHEN** the tag of an already seen release `acme/greeter@0.2.0` is moved to another commit on the origin
- **THEN** `update acme/greeter` and `update all` do not fail because of it; what they report is specified in updates ("Stable update never downgrades")

#### Scenario: Moved tag of a sibling
- **WHEN** the tag of installed `tilr/theme@0.1.0` moved on the origin and the user updates `tilr/store` from the same shared clone
- **THEN** the update of `tilr/store` succeeds and `tilr/theme` keeps its installed commit

#### Scenario: Moved tag on regenerate
- **WHEN** a published tag was moved to another commit and the user runs `catalog-update`
- **THEN** the published record is kept and a warning names the moved tag (catalog, "Published versions are immutable")

## REMOVED Requirements

### Requirement: Moved release tags are refused
**Reason**: Owner decision D5 (TILR-39): moved or retagged releases are not gitpm's problem; integrity comes only from a verified catalog.
**Migration**: None for users; scripts that expected `version_conflict` on a moved tag now see the update succeed or report up to date with a warning.
