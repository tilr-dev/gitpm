## MODIFIED Requirements

### Requirement: Metadata in the catalog
Because a catalog release carries the whole manifest (catalog, "Release record"), `catalog update` and `catalog register` SHALL publish the metadata; `catalog show` and `browse` SHALL show the metadata of each package's latest release by reading its `release.json` (bounded as in catalog, "Bounded overview"), and a missing or broken latest `release.json` SHALL only leave that package without metadata. The catalog plugin index format SHALL stay unchanged.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm catalog show` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`
