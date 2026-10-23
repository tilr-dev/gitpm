## MODIFIED Requirements

### Requirement: Install from the catalog
install-catalog SHALL install the exact requested release, latest if omitted. With build it SHALL verify archive SHA-256 and release file hashes and deploy without cloning; otherwise it SHALL verify the recorded Git commit, manifest and output hashes. Mismatches SHALL fail with catalog and unavailable artifacts with http, preserving the workspace. Only verified catalogs protect; build installs additionally retain catalog provenance.

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed
- **AND** installing from a second catalog is refused as specified in catalog ("One catalog per workspace")

#### Scenario: Catalog install over a direct install
- **WHEN** `acme/greeter` 0.1.0 was installed or downloaded with `install`/`download ./greeter` and the user runs `install-catalog acme/greeter` from a verified catalog
- **THEN** the installed revision must be the catalog's record of 0.1.0 (same source and commit, package folder, manifest and file hashes): the record gets `catalog` (a download is also enabled) and the result warns `acme/greeter is now protected by catalog LOCATION`
- **AND** another commit or other files fail with `catalog` ("… it cannot be protected by that catalog") and nothing changes; this is covered by `workspace::install_from_a_verified_catalog_protects_a_direct_install_of_its_release`

#### Scenario: Install from an unverified catalog
- **WHEN** the catalog has no `"verified": true` and the user runs `install-catalog acme/greeter`
- **THEN** the release is verified as above, the record has no `catalog`, and a Git installation later updates from the origin; a build installation follows its separately recorded catalog location

#### Scenario: Install a build with unavailable source
- **WHEN** a catalog release has a valid build and its source repository is unavailable
- **THEN** install-catalog deploys the verified archive output without a clone


### Requirement: Removing a package whose clone is gone
Uninstall and forget SHALL drop a Git record whose clone is gone, plus its deployed output, without confirmation or a forgotten record, subject to dependent guards. A build intentionally has no clone: forget SHALL retain its record and cached output for restore; uninstall SHALL remove the cache. Managed build output SHALL NOT count as lost Git work.

#### Scenario: Clone deleted by hand
- **WHEN** `packages/acme/greeter` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git
