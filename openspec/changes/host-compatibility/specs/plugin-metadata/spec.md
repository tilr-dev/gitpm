## MODIFIED Requirements

### Requirement: Reported shape
Single source for the reported shape: wherever metadata is reported (`list` `resolved[]`, `info`/`browse` `PackageSummary`, `CatalogRelease`, FFI records), all four keys and the manifest's `host` map SHALL be present: an absent string SHALL be `null`, an absent list `[]` and an absent `host` `{}`. In the raw state (`packages.<id>.metadata`, printed by `list`) the `metadata` object, when present, has all four keys. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Only a title
- **WHEN** a package has only `title`
- **THEN** `list` shows `"description": null`, `"provides": []`, `"tags": []`

#### Scenario: Host ranges reported
- **WHEN** a package's manifest has `"host": {"tilr": ">=3.0.0"}`
- **THEN** `list` `resolved[]` and the catalog summaries report `"host": {"tilr": ">=3.0.0"}`, and a package without the key reports `"host": {}`
- **AND** a catalog summary takes `host` from the manifest of the package's latest release record, where it also takes the other metadata (plugin-metadata, "Metadata in the catalog")
