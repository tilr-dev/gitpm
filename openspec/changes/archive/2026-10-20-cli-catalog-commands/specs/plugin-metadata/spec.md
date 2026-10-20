## MODIFIED Requirements

### Requirement: Lenient reading, strict authoring
`commit` and the release step of `fork --replace` SHALL refuse a metadata key of the wrong type with `invalid_manifest`. `install`, `install-catalog`, `update`, `update all`, `rollback`, `restore`, `fork`, `catalog update`, `catalog register` and `list` SHALL treat such a key (and `null`) as absent. `install`, `install-catalog`, `update` and `update all` SHALL warn on stderr and report it in `InstallResult.warnings` / `RevisionChange.warnings`; `rollback` and `restore` report no warnings.

#### Scenario: Warning text
- **WHEN** an install ignores a wrong-typed `tags`
- **THEN** stderr has `Warning: ID VERSION: plugin.json tags must be an array of strings; ignored`

#### Scenario: Old release with bad tags
- **WHEN** a release has `"tags": "ui"` and the user installs it
- **THEN** the install succeeds, `tags` is `[]` and a warning is printed
- **AND** this is covered by `wrong_typed_metadata_is_ignored_when_installing_and_refused_when_authoring` and `catalog_update_publishes_historical_releases_with_wrong_typed_metadata`

#### Scenario: Authoring with bad tags
- **WHEN** the author runs `gitpm commit REPO 0.2.0` with `"tags": "ui"`
- **THEN** the command fails with `invalid_manifest` and nothing is committed

### Requirement: Reported shape
Single source for the reported shape: wherever metadata is reported (`list` `resolved[]`, `catalog show`/`browse` `PackageSummary`, `CatalogRelease`, FFI records), all four keys SHALL be present: an absent string SHALL be `null` and an absent list SHALL be `[]`. In the raw state (`packages.<id>.metadata`, printed by `list`) the `metadata` object is always present and has all four keys. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Only a title
- **WHEN** a package has only `title`
- **THEN** `list` shows `"description": null`, `"provides": []`, `"tags": []`

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), `catalog update` and `catalog register` SHALL publish the metadata; `catalog show` and `browse` SHALL show the metadata of each package's latest release by reading its `release.json` (bounded as in catalog, "Bounded overview"), and a missing or broken latest `release.json` SHALL only leave that package without metadata. The catalog package index format SHALL stay unchanged.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm catalog show` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`
