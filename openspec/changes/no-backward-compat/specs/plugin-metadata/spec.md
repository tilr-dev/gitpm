## MODIFIED Requirements

### Requirement: Reported shape
Single source for the reported shape: wherever metadata is reported (`list` `resolved[]`, `info`/`browse` `PackageSummary`, `CatalogRelease`, FFI records), all four keys SHALL be present: an absent string SHALL be `null` and an absent list SHALL be `[]`. In the raw state (`packages.<id>.metadata`, printed by `list`) the `metadata` object, when present, has all four keys. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Only a title
- **WHEN** a package has only `title`
- **THEN** `list` shows `"description": null`, `"provides": []`, `"tags": []`

### Requirement: Metadata in state
Install and update SHALL store the installed revision's metadata in `packages.<id>.metadata`. `list` SHALL report the metadata of `resolved[]` from that record only and SHALL NOT read the clone; a record without `metadata` reports the absent shape ("Reported shape").

#### Scenario: Workspace from 0.4
- **WHEN** a state record has no `metadata` (as gitpm 0.4 wrote it)
- **THEN** `list` reports `title` and `description` as `null` and `provides` and `tags` as `[]`, without reading the clone
