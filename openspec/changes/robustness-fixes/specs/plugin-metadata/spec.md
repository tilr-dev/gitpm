## MODIFIED Requirements

### Requirement: Lenient reading, strict authoring
`commit` and the release step of `fork --replace` SHALL refuse a metadata key of the wrong type with `invalid_manifest`. `install`, `install-catalog`, `update`, `update all`, `rollback`, `restore`, `fork`, `catalog-update`, `register` and `list` SHALL treat such a key (and `null`) as absent. `install`, `install-catalog`, `update` and `update all` SHALL warn on stderr and report it in `InstallResult.warnings` / `RevisionChange.warnings`; `rollback` and `restore` report no warnings.

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

