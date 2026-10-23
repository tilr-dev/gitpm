## MODIFIED Requirements

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact release (latest if omitted), verifying the manifest and file hashes before any change. Git installs SHALL check out the recorded source commit; builds SHALL verify archive SHA-256 and deploy without cloning. Mismatches SHALL fail with `catalog`, unavailable artifacts with `http`. Only verified catalogs protect. Verified by `catalog_manifest_is_verified_before_install`.

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

#### Scenario: Catalog verification before mutation
- **WHEN** `Manager::install_from_catalog` selects a Git release
- **THEN** the committed package folder, whole manifest and output file hashes are verified against the catalog before any workspace change
- **AND** verification is covered by `workspace::install_from_catalog_verifies_and_check_compares`

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a recorded Git package whose own or shared clone is gone SHALL succeed without confirmation, drop the record and deployed folder, keep no forgotten record, and retain dependent guards (dependencies, "Dependents guard removal"). Builds intentionally have no clone and SHALL follow the cache lifecycle below. Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `packages/acme/greeter` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git

#### Scenario: Managed build output is not lost Git work
- **WHEN** a build is forgotten or uninstalled
- **THEN** managed cached output needs no lost-work confirmation; forget retains its record and cached output for restore, while uninstall removes the cache, subject to dependent guards
