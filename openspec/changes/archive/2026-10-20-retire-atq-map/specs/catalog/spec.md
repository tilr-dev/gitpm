## ADDED Requirements

### Requirement: Local release history remains bounded in execution time
For the local integration fixture containing one package with 200 committed releases `1.0.0` through `1.199.0`, catalog generation, catalog overview, catalog installation and direct installation SHALL each complete in less than 60 seconds. This is the local fixture's acceptance budget, not a bound on remote transport latency. Verified by `uc_061:two_hundred_versions`.

#### Scenario: Two hundred local releases
- **WHEN** the local fixture creates the 200 releases and runs `catalog-update`, `info`, `install-catalog` and direct `install` separately
- **THEN** each command completes in less than 60 seconds, the generated catalog contains 200 versions, the overview reports `1.199.0` as latest and 200 versions, and both installation forms select `1.199.0`
