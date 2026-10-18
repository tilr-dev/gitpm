## MODIFIED Requirements

### Requirement: Reported shape
Single source for the reported shape: wherever metadata is reported (`list` `resolved[]`, `info`/`browse` `PackageSummary`, `CatalogRelease`, FFI records), all four keys SHALL be present: an absent string SHALL be `null` and an absent list SHALL be `[]`. In the raw state (`packages.<id>.metadata`, printed by `list`) the `metadata` object is always present and has all four keys. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Only a title
- **WHEN** a package has only `title`
- **THEN** `list` shows `"description": null`, `"provides": []`, `"tags": []`

### Requirement: Metadata in state
Install and update SHALL store the installed revision's metadata in `packages.<id>.metadata`; the field is required in every record of state format 3. `list` SHALL report the metadata of `resolved[]` from that record only and SHALL NOT read the clone. A record without `metadata` SHALL be refused with `invalid_data` and `state.json` MUST NOT be rewritten. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides` and `model::state_requires_core_fields`.

#### Scenario: Workspace from 0.4
- **WHEN** a record of a format-3 `state.json` has no `metadata` (for example edited by hand)
- **THEN** `list` and every other operation on the workspace fail with `invalid_data` ("missing field `metadata`") and `state.json` keeps its bytes

### Requirement: Filter by provided service
`list --provides SERVICE` SHALL keep only the packages whose `provides` contains `SERVICE` in `packages` and `resolved`; `replacements` and `forgotten` SHALL be printed unfiltered, and inactive (replaced, disabled) providers SHALL be kept because `list` is a status view.

#### Scenario: Replaced original still listed
- **WHEN** `me/theme` replaces `tilr/theme`, both providing `theme`
- **THEN** `list --provides theme` lists both, with `active` false for `tilr/theme`

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), `catalog-update` and `register` SHALL publish the metadata; `info` and `browse` SHALL show the metadata of each package's latest release by reading its `release.json` (bounded as in catalog, "Bounded overview"), and a missing or broken latest `release.json` SHALL only leave that package without metadata. The catalog package index format SHALL stay unchanged.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm info` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`
