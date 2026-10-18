## ADDED Requirements

### Requirement: Nothing is built, executed or uploaded
gitpm SHALL only clone Git repositories, copy their package dist folders, write local catalogs and, for the author operations (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload. Writing a catalog to an HTTP(S) location SHALL fail with `invalid_argument` and a message that names no command.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog-update` against an HTTPS catalog location
- **THEN** the library fails with `invalid_argument` ("writing a catalog requires a local folder; HTTP(S) catalogs are read-only") and nothing is uploaded; the CLI may add a hint naming the command

### Requirement: A configured catalog location must exist
A local catalog location given in `Options.catalog` that does not exist SHALL make `Manager::open_with` fail with `invalid_argument` ("catalog location PATH does not exist") before anything is created; there SHALL be no fallback to "no catalog". Free functions that read a catalog fail the same way. Only the catalog writers (`catalog-update`, `register`) create a missing local catalog. Verified by `workspace::a_missing_catalog_location_is_refused_before_creating_anything`.

#### Scenario: Typo in the catalog path
- **WHEN** a host opens a Manager on a new workspace root with `Options.catalog` `./catlog` that does not exist
- **THEN** opening fails with `invalid_argument` and neither the workspace root nor its lock file is created

#### Scenario: New catalog
- **WHEN** the author runs `register` with a local catalog folder that does not exist yet
- **THEN** the folder is created and the release is written
