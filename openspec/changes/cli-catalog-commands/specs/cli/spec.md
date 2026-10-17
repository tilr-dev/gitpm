## ADDED Requirements

### Requirement: Catalog command group
The CLI SHALL provide the catalog operations as subcommands of `gitpm catalog`: `show` (text overview), `links` (JSON links of one level), `update [--sources FILE]` (generate from Git release tags), `migrate` (v1 to v2 tree) and `register GIT_SOURCE --ref REF [--package ID] [--build-url URL --build-sha256 SHA]` (one committed version). Each SHALL behave and print as its operation does under the `catalog` spec.

#### Scenario: Generate a catalog with the group name
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0 and the user runs `gitpm --catalog ./catalog catalog update`
- **THEN** stdout is `Catalog ready: 2 release(s) checked from sources.` and the exit status is 0

#### Scenario: Register with the group name
- **WHEN** the user runs `gitpm --catalog ./catalog catalog register ./greeter --ref acme/greeter@0.1.0`
- **THEN** the release is added and printed as pretty JSON

### Requirement: Catalog subcommands need a catalog
Every `catalog` subcommand SHALL need a catalog location; without `--catalog` or `GITPM_CATALOG` it SHALL fail with `catalog SUB needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` and create nothing.

#### Scenario: Missing catalog names the subcommand
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm catalog show`
- **THEN** it fails with `catalog show needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` and no file is created

#### Scenario: Writing to an HTTP catalog
- **WHEN** the user runs `gitpm --catalog https://pkg.example/catalog.json catalog update`
- **THEN** the command refuses, because HTTP(S) catalogs are read-only, and nothing is uploaded

### Requirement: Deprecated catalog command names
The former spellings `info`, `catalog` without a subcommand, `catalog-update`, `migrate-catalog` and `register` SHALL keep working as aliases of `catalog show`, `catalog links`, `catalog update`, `catalog migrate` and `catalog register`, with identical stdout and exit status, and SHALL print one line `Warning: \`gitpm OLD\` is deprecated; use \`gitpm catalog NEW\`` on stderr. Options SHALL be accepted under an alias wherever they are accepted under its new name.

#### Scenario: Old spelling still works
- **WHEN** the user runs `gitpm --catalog ./catalog catalog-update`
- **THEN** stdout equals that of `gitpm --catalog ./catalog catalog update`
- **AND** stderr contains ``Warning: `gitpm catalog-update` is deprecated; use `gitpm catalog update` ``

#### Scenario: Bare catalog
- **WHEN** the user runs `gitpm --catalog ./catalog catalog`
- **THEN** it prints the JSON links of the catalog root level, as `catalog links` does, plus the deprecation warning on stderr

### Requirement: Catalog group usage and errors
`gitpm catalog --help` SHALL print the usage blocks of every catalog subcommand, and `gitpm catalog SUB --help` only that subcommand's block. An unknown word after `catalog` SHALL fail with `unknown catalog command: X; use gitpm catalog --help`, and wrong argument counts or misplaced options SHALL fail naming the two-word command (for example `--sources requires catalog update`), in both cases before opening a workspace or catalog.

#### Scenario: Unknown subcommand
- **WHEN** the user runs `gitpm catalog publish`
- **THEN** it fails with `unknown catalog command: publish; use gitpm catalog --help`, exit status 1, and nothing is created

#### Scenario: Subcommand help
- **WHEN** the user runs `gitpm catalog register --help`
- **THEN** only the `catalog register` usage block is printed and the exit status is 0

#### Scenario: Misplaced sources flag
- **WHEN** the user runs `gitpm catalog show --sources list.txt`
- **THEN** it fails with `--sources requires catalog update`
