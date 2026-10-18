## MODIFIED Requirements

### Requirement: Published versions are immutable
A published version SHALL NOT change. `register` SHALL compare the whole release JSON except `reference` and fail with `version_conflict` ("version ID@V already exists and is immutable"), checking before any write so a rejected register writes nothing. `catalog-update` SHALL never replace a published record: when the regenerated release differs from it (manifest, source, commit, files or path, a moved tag included), it SHALL keep the published record, warn and go on, never failing with `version_conflict` for it. A package ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable` and `catalog_update_keeps_published_records_when_tags_move`.

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

