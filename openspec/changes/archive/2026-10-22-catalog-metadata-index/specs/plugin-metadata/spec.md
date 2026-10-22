## MODIFIED Requirements

### Requirement: Metadata in the catalog
`catalog update` and `catalog register` SHALL publish the latest release's metadata in the package index, under the catalog lock in the same atomic write. `catalog show` and `browse` SHALL read that summary without reading `release.json`, with at most one index request per listed package and the existing bounded overview. A missing or broken summary SHALL only leave its package without metadata. Installs SHALL keep verifying against `release.json`.

#### Scenario: info shows metadata
- **WHEN** the latest release of `tilr/theme` has a title and provides
- **THEN** `gitpm catalog show` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`

#### Scenario: Overview without per-package release reads
- **WHEN** an HTTPS catalog lists 40 packages
- **THEN** `browse` shows their metadata with at most 40 index requests and without requesting any `release.json`

#### Scenario: One broken summary
- **WHEN** the index summary of one listed package is missing or invalid
- **THEN** the overview shows that package without metadata and the others normally
