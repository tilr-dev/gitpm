## MODIFIED Requirements

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact catalog release (latest if omitted): clone its source, check out the recorded commit, and verify that the manifest and the package dist file hashes equal the catalog's before any workspace change. A mismatch SHALL fail with `catalog`. Only a verified catalog makes the package protected (catalog, "One catalog per workspace"). Verified by `catalog_manifest_is_verified_before_install` and `workspace::install_from_catalog_verifies_and_check_compares`.

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
- **THEN** the release is verified as above, the record has no `catalog`, and `update acme/greeter` later reads the origin
