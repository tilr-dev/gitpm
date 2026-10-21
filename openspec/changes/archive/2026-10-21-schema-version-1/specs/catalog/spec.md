## RENAMED Requirements

- FROM: `### Requirement: Catalog v2 is a tree of static files`
- TO: `### Requirement: Catalog is a tree of static files`

## MODIFIED Requirements

### Requirement: Catalog is a tree of static files
A catalog SHALL be a tree of JSON files holding only links: `catalog.json` and directory files `{"schema": 1, "directories": {…}}`, a catalog package index `namespaces/<ns>/<name>/plugin.json` `{"schema": 1, "id", "source", "versions"}` per package, and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog package index has `"schema": 2`
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported package schema" for the index; a `state.json` of another format is `unsupported` instead, see errors)

#### Scenario: v1 catalog
- **WHEN** the catalog location is a monolithic v1 `catalog.json`
- **THEN** every catalog read fails with `catalog` and nothing is written; the user regenerates the catalog with `catalog update`

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `catalog show`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`
