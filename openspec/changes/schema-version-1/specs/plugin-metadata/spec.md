## MODIFIED Requirements

### Requirement: Metadata in state
Install and update SHALL store the installed revision's metadata in `packages.<id>.metadata`; the field is required in every record of state format 1. `list` SHALL report the metadata of `resolved[]` from that record only and SHALL NOT read the clone. A record without `metadata` SHALL be refused with `invalid_data` and `state.json` MUST NOT be rewritten. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides` and `model::state_requires_core_fields`.

#### Scenario: Workspace from 0.4
- **WHEN** a record of a format-1 `state.json` has no `metadata` (for example edited by hand)
- **THEN** `list` and every other operation on the workspace fail with `invalid_data` ("missing field `metadata`") and `state.json` keeps its bytes
