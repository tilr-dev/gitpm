## MODIFIED Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL be a download plus enabling the package and the dependencies it downloaded, in one transaction: it clones the Git source (path or URL), selects the newest valid release tag that is not a pre-release (package-ids), checks it out, verifies the manifest and deploys its package dist. Untagged commits SHALL be ignored; a source without release tags SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

#### Scenario: Install enables pulled dependencies
- **WHEN** `install-catalog app` pulls the missing `lib` from the catalog
- **THEN** both are recorded `enabled: true` and deployed in the same transaction

#### Scenario: Pre-release tag is not picked
- **WHEN** a repository has `acme/greeter@1.2.0` and the newer `acme/greeter@1.3.0-beta.1`
- **THEN** `install` installs 1.2.0; `install SOURCE --ref acme/greeter@1.3.0-beta.1` installs the beta
