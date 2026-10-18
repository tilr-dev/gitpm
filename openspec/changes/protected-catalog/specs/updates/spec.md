## ADDED Requirements

### Requirement: Protected packages update through their catalog
A protected package (recorded `catalog`) SHALL update only to versions its verified catalog lists, read from the recorded location whatever `Options.catalog` is; the new release's commit, manifest and file hashes SHALL be checked against the release record before any change (`catalog` on a mismatch). `update ID --detach` SHALL drop the protection and update from the origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `acme/greeter` 0.2.0 has the tag `acme/greeter@0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update acme/greeter` prints `acme/greeter up to date (0.2.0)` and `update all` lists it in `unchanged`

#### Scenario: Hash mismatch on update
- **WHEN** the verified catalog lists 0.3.0 but the files at the recorded commit do not match its hashes
- **THEN** `update acme/greeter` fails with `catalog` and the workspace is unchanged

#### Scenario: Detach
- **WHEN** the user runs `update acme/greeter --detach`
- **THEN** the record loses `catalog` and the package moves to the origin's newest release 0.3.0

## MODIFIED Requirements

### Requirement: Updates need no catalog
`update ID` (any mode) and `update all` SHALL read releases and branches from the origin of the package's clone (set from `plugins.<id>.source`) and SHALL NOT read a catalog for unprotected packages, so they work without one (protected ones: "Protected packages update through their catalog"); `check` reads the same origin for packages the catalog does not list (catalog). Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `acme/greeter` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update acme/greeter` prints `acme/greeter updated 0.1.0 → 0.2.0`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `acme/greeter would be updated 0.1.0 → 0.2.0` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `acme/greeter` says `update available` with `0.2.0` from the origin, and nothing in the workspace changes

