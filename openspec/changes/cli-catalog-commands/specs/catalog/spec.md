## MODIFIED Requirements

### Requirement: Bounded overview
`catalog show` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> catalog show`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 packages
- **THEN** `catalog show` shows 40 and the limit line

#### Scenario: Overview lines
- **WHEN** the user runs `gitpm --catalog ./catalog catalog show` on a local v2 catalog
- **THEN** it prints `Catalog: LOCATION`, `Type: local folder/JSON`, `Format: v2`, the directory tree and for each package `ID — latest: V, versions: N` followed by its `Git: SOURCE` line
- **AND** it changes no file of the catalog or the workspace
