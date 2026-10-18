## MODIFIED Requirements

### Requirement: Bounded overview
`catalog show` and `browse` SHALL walk the catalog depth-first, follow only valid links and stop at 40 entries, printing `Overview limit: 40 entries. To browse one branch: gitpm --catalog <branch location> catalog show`. Verified by `catalog::browse_follows_only_valid_links_and_is_bounded`.

#### Scenario: Large catalog
- **WHEN** the catalog has more than 40 packages
- **THEN** `catalog show` shows 40 and the limit line
