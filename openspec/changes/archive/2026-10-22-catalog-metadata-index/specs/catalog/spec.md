## MODIFIED Requirements

### Requirement: Catalog is a tree of static files
A catalog SHALL be a tree of JSON files: schema-1 directory links, a schema-1 package index `namespaces/<ns>/<name>/plugin.json` with `id`, `source`, `versions` and `latest`, and one `versions/<V>/release.json` per version. `latest` SHALL summarize the selected release's version and metadata. Lookup by ID SHALL NOT walk the tree. Any other schema value, a v1 monolithic `catalog.json` included, SHALL be refused with `catalog`.

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

#### Scenario: Release summary in the index
- **WHEN** a catalog is generated and the latest release of `tilr/theme` has a title and provides
- **THEN** `namespaces/tilr/theme/plugin.json` holds `latest {version, title, description, provides, tags}`, equal to the normalized manifest metadata in its `release.json`; directory files `{"schema": 1, "directories": {…}}` carry only links

#### Scenario: Summary selection and regeneration
- **WHEN** a catalog is generated or a release is registered, including an older version or an already published version
- **THEN** the index summary matches the newest stable release, or the newest prerelease if no stable release exists, and its schema number remains unchanged
- **AND** a failed generation leaves the existing index unchanged
