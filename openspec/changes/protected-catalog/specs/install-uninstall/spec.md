## MODIFIED Requirements

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact catalog release (latest if omitted): clone, check out the recorded commit and verify that manifest and package dist file hashes equal the catalog's before any change. A mismatch SHALL fail with `catalog`. Only a verified catalog makes the package protected (catalog, "One catalog per workspace"). Verified by `catalog_manifest_is_verified_before_install`.

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed
- **AND** installing from a second catalog is refused as specified in catalog ("One catalog per workspace")

#### Scenario: Install from an unverified catalog
- **WHEN** the catalog has no `"verified": true` and the user runs `install-catalog acme/greeter`
- **THEN** the release is verified as above, the record has no `catalog`, and `update acme/greeter` later reads the origin
