## MODIFIED Requirements

### Requirement: Catalog v2 is a tree of static files
A catalog SHALL be a tree of JSON files holding only links and summaries: `catalog.json` and directory files `{"schema": 2, "directories": {…}}`, a catalog package index `namespaces/<ns>/<name>/plugin.json` per package (`schema` 3, `id`, `source`, `versions` and `latest`, the summary of the newest release: `version`, `title`, `description`, `provides`, `tags`, `host`), and one `versions/<V>/release.json` per version. Lookup by ID SHALL NOT walk the tree. Any other schema value, an index of schema 2 and a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`; there is no reader for older layouts.

#### Scenario: Unknown schema
- **WHEN** a directory file or catalog package index has a schema value this gitpm does not write (for example an index of `"schema": 2` from gitpm 0.5)
- **THEN** reading it fails with `catalog` ("unsupported catalog schema", or "unsupported package schema" for the index; a `state.json` of another format is `unsupported` instead, see errors); the user regenerates the catalog with `catalog update`

#### Scenario: Too deep
- **WHEN** the directory links of a catalog nest more than 8 levels
- **THEN** `catalog show`/`browse` fail with `catalog` ("catalog nesting is too deep")

#### Scenario: Direct lookup
- **WHEN** the directory indexes are damaged but `namespaces/dan/demo1/` is intact
- **THEN** `install-catalog dan/demo1` still finds the release
- **AND** this is covered by `catalog::v2_layout_links_directories_bottom_up` and `split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes`

#### Scenario: Release summary in the index
- **WHEN** a catalog is generated and the latest release of `tilr/theme` has a title, provides and `"host": {"tilr": ">=3.0.0"}`
- **THEN** `namespaces/tilr/theme/plugin.json` holds `latest` with that version, title, provides and host, equal to the manifest in its `release.json`; directory files carry no summaries
