## MODIFIED Requirements

### Requirement: Catalog command group
The CLI SHALL provide the catalog operations as subcommands of `gitpm catalog`: `show` (text overview), `links` (JSON links of one level), `update [--sources FILE]` (generate from Git release tags) and `register GIT_SOURCE --ref REF [--package ID] [--build-url URL --build-sha256 SHA]` (one committed version). Each SHALL behave and print as its operation does under the `catalog` spec.

#### Scenario: Generate a catalog with the group name
- **WHEN** `sources.txt` lists `./greeter` with tags `v0.1.0` and `v0.2.0` and the user runs `gitpm --catalog ./catalog catalog update`
- **THEN** stdout is `Catalog ready: 2 release(s) checked from sources.` and the exit status is 0

#### Scenario: Register with the group name
- **WHEN** the user runs `gitpm --catalog ./catalog catalog register ./greeter --ref v0.1.0`
- **THEN** the release is added and printed as pretty JSON
