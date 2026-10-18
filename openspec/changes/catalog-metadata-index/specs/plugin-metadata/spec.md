## MODIFIED Requirements

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), catalog generation and registration SHALL publish the metadata and SHALL write the latest release's metadata into the catalog plugin index. The catalog overview (`browse`) SHALL show each package's metadata from that index without reading `release.json`, bounded as in catalog ("Bounded overview"), and a missing or broken summary SHALL only leave that package without metadata.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** the catalog overview prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`

#### Scenario: Overview without per-package release reads
- **WHEN** an HTTPS catalog lists 40 packages
- **THEN** `browse` shows their metadata without requesting any `release.json`

#### Scenario: One broken summary
- **WHEN** the index summary of one listed package is missing or invalid
- **THEN** the overview shows that package without metadata and the others normally
