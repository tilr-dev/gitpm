## MODIFIED Requirements

### Requirement: Published versions are immutable
A published version SHALL NOT change; every version is checked before any write, so a rejected update writes nothing. `register` SHALL compare the whole release JSON except `reference` and fail with `version_conflict` ("version ID@V already exists and is immutable"). A package ID SHALL stay bound to the source that first registered it ("registered from a different source" → `catalog`). Verified by `catalog::published_versions_are_immutable`.

#### Scenario: Register with another build
- **WHEN** a published version is registered again with different `--build-url`
- **THEN** register fails with `version_conflict` ("version ID@V already exists and is immutable")

#### Scenario: Regenerate
- **WHEN** `catalog-update` regenerates a published version
- **THEN** it keeps the published record unchanged (`reference` and `build` included) and never replaces it
- **AND** when the tag of a published version now points at another commit, it keeps the record, prints `Warning: release ID@V: tag now points to COMMIT; the published record is kept` and does not fail (package-ids, "Tags are not policed; integrity via protected catalog")

