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

### Requirement: Nothing is built, executed or uploaded
gitpm SHALL only clone Git repositories, copy their package dist folders, write local catalogs and, for the author operations (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload. Writing a catalog to an HTTP(S) location SHALL fail with `invalid_argument` and a message that names no command.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog update` against an HTTPS catalog location
- **THEN** the library fails with `invalid_argument` ("writing a catalog requires a local folder; HTTP(S) catalogs are read-only") and nothing is uploaded; the CLI adds a `Hint:` line naming `gitpm catalog update` with a local folder
