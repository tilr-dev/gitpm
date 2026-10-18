## MODIFIED Requirements

### Requirement: Catalog generated from Git release tags
`catalog-update [--sources FILE]` SHALL read the repositories listed in the sources file (one per line, relative to the file; blank and `#` lines skipped), discover every release tag, pin each tag's commit, hash its package dist and write the tree. For a multi-package repository it SHALL publish only the packages listed in `gitpm.json` at HEAD. Broken historical tags are skipped as in package-ids. It SHALL validate every source before writing anything and retain releases of removed sources.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog-update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0
- **THEN** `catalog-update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_keeps_published_records_when_tags_move` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

#### Scenario: Package removed from gitpm.json at HEAD
- **WHEN** a multi-package repository tagged `tilr/old@0.1.0` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `tilr/old` yet
- **THEN** `catalog-update` publishes the releases of the packages listed at HEAD and no release of `tilr/old`

#### Scenario: Duplicate source lines
- **WHEN** the sources file lists the same repository twice, also in another spelling of its path
- **THEN** `catalog-update` reads it once and indexes each release once

### Requirement: Published versions are immutable
A published version SHALL NOT change; every version is checked before any write, so a rejected `register` or `catalog-update` writes nothing. `register` SHALL compare the whole release JSON except `reference` and fail with `version_conflict` ("version ID@V already exists and is immutable"). `catalog-update` SHALL never replace a published record: when the regenerated release differs from it (manifest, source, commit, files or path, a moved tag included), it SHALL keep the published record, warn and go on, never failing with `version_conflict` for it. A package ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable` and `catalog_update_keeps_published_records_when_tags_move`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog-update` regenerates a published version
- **THEN** it keeps the published record unchanged (`reference` and `build` included) and never replaces it
- **AND** when the tag of a published version now points at another commit, it keeps the record, prints `Warning: release ID@V: tag now points to COMMIT; the published record is kept` and does not fail (package-ids, "Tags are not policed; integrity via protected catalog")

#### Scenario: Manifest or files differ from the published record
- **WHEN** the release commit of a published version is unchanged but its regenerated manifest or file hashes differ from the published record (for example after a manual edit of `release.json`)
- **THEN** `catalog-update` keeps the published record, prints `Warning: release ID@V differs from the published record; the published record is kept`, exits 0 and publishes the other versions
