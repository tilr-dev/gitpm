# plugin-metadata Specification

## Purpose

Defines the optional descriptive keys of `plugin.json` (`title`, `description`, `provides`, `tags`): gitpm stores and reports them without interpreting them, reads them leniently and validates them strictly only when authoring.

## Requirements

### Requirement: Metadata keys
`plugin.json` MAY carry `title` (string), `description` (string), `provides` (string or array of strings; a string SHALL be read as a one-element array) and `tags` (array of strings). gitpm SHALL only store and report them and SHALL NOT interpret them (no exclusivity, no slots). Verified by `model::manifest_metadata_is_typed_normalized_and_written_back_as_it_was`.

#### Scenario: String provides
- **WHEN** a manifest has `"provides": "theme"`
- **THEN** gitpm reports `provides` as `["theme"]` and keeps the string form in the file

### Requirement: Lenient reading, strict authoring
`commit` SHALL refuse a metadata key of the wrong type with `invalid_manifest`. `install`, `update`, `rollback`, `fork`, `catalog-update` and `list` SHALL treat such a key (and `null`) as absent; `install`, `update` and `update all` SHALL warn on stderr (`Warning: ID VERSION: plugin.json tags must be an array of strings; ignored`) and the library SHALL report it in `InstallResult.warnings` / `RevisionChange.warnings`; `rollback` reports no warnings (its `warnings` is always empty).

#### Scenario: Old release with bad tags
- **WHEN** a release has `"tags": "ui"` and the user installs it
- **THEN** the install succeeds, `tags` is `[]` and a warning is printed
- **AND** this is covered by `wrong_typed_metadata_is_ignored_when_installing_and_refused_when_authoring` and `catalog_update_publishes_historical_releases_with_wrong_typed_metadata`

#### Scenario: Authoring with bad tags
- **WHEN** the author runs `gitpm commit REPO 0.2.0` with `"tags": "ui"`
- **THEN** the command fails with `invalid_manifest` and nothing is committed

### Requirement: Metadata survives rewrites
`commit`, `fork` and `fork --replace` SHALL rewrite `version`, `id`, `forkOf`, `replaces` and dependencies but SHALL keep the metadata keys exactly as they were. (The `"title": "NAME"` that `init` writes is specified in authoring.) Verified by `fork_and_commit_carry_the_metadata_unchanged`.

#### Scenario: Fork keeps the title
- **WHEN** `tilr/theme` with `"title": "Theme"` is forked as `me/theme`
- **THEN** the fork's manifest still has `"title": "Theme"`

### Requirement: Reported shape
Wherever metadata is reported (`list` `resolved[]`, `info`/`browse` `PluginSummary`, `CatalogRelease`, FFI records), all four keys SHALL be present: an absent string SHALL be `null` and an absent list SHALL be `[]`. In the raw state (`plugins.<id>.metadata`, also printed by `list`) the `metadata` object, when present, has all four keys, but it is absent in a record written by gitpm 0.4 or earlier. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Only a title
- **WHEN** a package has only `title`
- **THEN** `list` shows `"description": null`, `"provides": []`, `"tags": []`

### Requirement: Metadata in state
Install and update SHALL store the installed revision's metadata in `plugins.<id>.metadata`; for a record written by an older gitpm without it, `list` SHALL read the metadata for `resolved[]` from the manifest at the installed commit in its clone (the raw `plugins.<id>` stays without `metadata`).

#### Scenario: Workspace from 0.4
- **WHEN** a 0.4 state record has no `metadata`
- **THEN** `list` reports the metadata read from the clone

### Requirement: Filter by provided service
`list --provides SERVICE` SHALL keep only the packages whose `provides` contains `SERVICE` in `plugins` and `resolved`; `replacements` and `forgotten` SHALL be printed unfiltered, and inactive (replaced, disabled) providers SHALL be kept because `list` is a status view.

#### Scenario: Replaced original still listed
- **WHEN** `me/theme` replaces `tilr/theme`, both providing `theme`
- **THEN** `list --provides theme` lists both, with `active` false for `tilr/theme`

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), `catalog-update` and `register` SHALL publish the metadata; `info` and `browse` SHALL show the metadata of each plugin's latest release by reading its `release.json` (bounded as in catalog, "Bounded overview"), and a missing or broken latest `release.json` SHALL only leave that plugin without metadata. The plugin index format SHALL stay unchanged. Verified by `catalog_releases_carry_and_show_the_metadata`.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm info` prints `Title: Theme` and `Provides: theme` under the plugin
