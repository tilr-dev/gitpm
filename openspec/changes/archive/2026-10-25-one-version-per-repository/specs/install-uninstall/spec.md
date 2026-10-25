## MODIFIED Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the newest valid release tag that is not a pre-release (package-ids), check it out detached, verify the manifest and deploy its package dist; missing dependencies are installed as in dependencies. Untagged commits SHALL be ignored; a source without release tags, or with pre-release tags only, SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `v0.9.0` and `v0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

#### Scenario: Install enables pulled dependencies
- **WHEN** `install-catalog app` pulls the missing `lib` from the catalog
- **THEN** both are recorded `enabled: true` and deployed in the same transaction: an install is a download plus enabling the package and the dependencies it downloaded

#### Scenario: Forced install next to a disabled dependency
- **WHEN** `lib` is installed but disabled and the user runs `install ./app --force` (`Manager::install_package_with` with `force`; `install-catalog app --force`, `install_from_catalog_with`), where `app` requires `lib ^1`
- **THEN** `app` is recorded enabled and deployed although its dependency is not active (dependencies, "enable needs active dependencies"), `lib` stays disabled, missing dependencies are installed as usual, and `list` reports `app`'s dependency on `lib` unsatisfied (`forced_install_and_install_over_a_download`)

#### Scenario: Pre-release tag is not picked
- **WHEN** a repository has `v1.2.0` and the newer `v1.3.0-beta.1`
- **THEN** `install` installs 1.2.0; `install SOURCE --ref v1.3.0-beta.1` installs the beta (pinned, as every `--ref` tag)
