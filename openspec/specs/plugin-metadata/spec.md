# plugin-metadata Specification

## Purpose

Defines the optional descriptive keys of the manifest (`title`, `description`, `provides`, `tags`): gitpm stores and reports them without interpreting them, reads them leniently and validates them strictly only when authoring.

## Requirements

### Requirement: Metadata keys
The manifest MAY carry `title` (string), `description` (string), `provides` (string or array of strings; a string SHALL be read as a one-element array) and `tags` (array of strings). gitpm SHALL only store and report them and SHALL NOT interpret them (no exclusivity, no slots). Verified by `model::manifest_metadata_is_typed_normalized_and_written_back_as_it_was`.

#### Scenario: String provides
- **WHEN** a manifest has `"provides": "theme"`
- **THEN** gitpm reports `provides` as `["theme"]` and keeps the string form in the file

### Requirement: Lenient reading, strict authoring
`commit` SHALL refuse a metadata key of the wrong type with `invalid_manifest`. `install`, `install-catalog`, `update`, `update all`, `rollback`, `restore`, `fork` (the release step of `fork --replace` included), `catalog-update`, `register` and `list` SHALL treat such a key (and `null`) as absent. `install`, `install-catalog`, `update` and `update all` SHALL warn on stderr and report it in `InstallResult.warnings` / `RevisionChange.warnings`; `rollback` and `restore` report no warnings.

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

### Requirement: Metadata survives rewrites
`commit`, `fork` and `fork --replace` SHALL rewrite `version`, `id`, `forkOf`, `replaces` and dependencies but SHALL keep the metadata keys exactly as they were, in the position the unknown-key rule gives them (package-manifest, "Unknown keys are preserved"). (The `"title": "NAME"` that `init` writes is specified in authoring.) Verified by `fork_and_commit_carry_the_metadata_unchanged`.

#### Scenario: Fork keeps the title
- **WHEN** `tilr/theme` with `"title": "Theme"` is forked as `me/theme`
- **THEN** the fork's manifest still has `"title": "Theme"`

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
