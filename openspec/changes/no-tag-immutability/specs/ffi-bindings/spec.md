## ADDED Requirements

### Requirement: Tag warnings in records
The warnings of moved and retracted release tags SHALL reach the host: `RevisionChange.warnings` for `update ID`, and the `warnings` of each `UpdateAllPlan.unchanged` entry for `update all`. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Retracted release from Swift
- **WHEN** the origin deleted the tag of the installed release and a host calls `updateAll`
- **THEN** the package is in `unchanged` with the warning `installed release ID@V no longer exists on the origin`
