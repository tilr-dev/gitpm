## MODIFIED Requirements

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), `catalog update` and `catalog register` SHALL publish the metadata and SHALL write the latest release's metadata and `host` map into the catalog package index, in the same locked, atomic write as the index. `catalog show` and `browse` SHALL show each package's metadata and `host` from that index without reading any `release.json`, bounded as in catalog ("Bounded overview"): at most one index request per listed package. A missing or broken summary SHALL only leave that package without metadata. Installs keep verifying against `release.json`; the summary is for display only.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm catalog show` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`

#### Scenario: Overview without per-package release reads
- **WHEN** an HTTPS catalog lists 40 packages
- **THEN** `browse` shows their metadata and `host` with at most 40 index requests and without requesting any `release.json`

#### Scenario: One broken summary
- **WHEN** the index summary of one listed package is missing or invalid
- **THEN** the overview shows that package without metadata and the others normally
