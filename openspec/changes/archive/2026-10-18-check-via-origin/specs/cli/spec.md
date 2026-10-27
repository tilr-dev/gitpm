## MODIFIED Requirements

### Requirement: Commands that need a catalog say so
Catalog commands (`info`, `catalog`, `catalog-update`, `register`) and `install-catalog` SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`) and create nothing; `check` works without one. A given location SHALL be passed to the library unchanged, also for `install`. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: Catalog command without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm info`
- **THEN** it fails with `info needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`, exit status 1, and no file is created
- **AND** which operations need a catalog is specified in catalog ("A catalog is optional")

#### Scenario: Catalog path that does not exist
- **WHEN** the user runs `gitpm --catalog ./missing install ./greeter`
- **THEN** it fails with the library's `invalid_argument` (catalog, "A configured catalog location must exist"), prints `Error kind: invalid_argument`, exits with status 1 and creates nothing

#### Scenario: check without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm check` on a workspace with packages
- **THEN** it prints the check table from each package's origin and exits 0
