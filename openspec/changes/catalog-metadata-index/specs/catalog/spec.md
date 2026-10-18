## MODIFIED Requirements

### Requirement: Catalog v2 is a tree of static files
A v2 catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a catalog plugin index `namespaces/<ns>/<name>/plugin.json` per package (`schema`, `id`, `source`, `versions` and the `latest` release summary; legacy unscoped IDs under `legacy/<id>/`), and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Unknown schema values SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog plugin index has a schema value this gitpm does not write
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported plugin schema" for the index; a newer `state.json` format is `unsupported` instead, see errors)

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** the catalog overview fails with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** installing `dan/demo1` from the catalog still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

#### Scenario: Release summary in the index
- **WHEN** a catalog is generated and the latest release of `tilr/theme` has a title and provides
- **THEN** `namespaces/tilr/theme/plugin.json` holds `latest` with that version, title and provides, equal to the manifest in its `release.json`
