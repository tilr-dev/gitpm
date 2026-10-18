## ADDED Requirements

### Requirement: Download without enabling
`download SOURCE [ID] [--package ID] [--ref REF | --dev]` (`Manager::download`) and `download-catalog ID [--version V]` (`Manager::download_from_catalog`) SHALL clone, verify and record a package exactly as `install`/`install-catalog` do, but with `enabled: false` and nothing deployed; missing dependencies SHALL be downloaded the same way, disabled. The CLI prints `downloaded ID@V from SOURCE (disabled)` per package. Verified by `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Download with a missing dependency
- **WHEN** `app` requires `lib ^1`, only the catalog has `lib`, and the user runs `gitpm download ./app`
- **THEN** both are recorded with `enabled: false`, neither `dist/app/` nor `dist/lib/` exists, and the output has one `downloaded …` line per package

#### Scenario: Enable after download
- **WHEN** the user then runs `enable app`
- **THEN** it fails with `dependency_conflict` naming `lib` as disabled (dependencies, "enable needs active dependencies"), while `enable lib` followed by `enable app` succeeds

## MODIFIED Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL be a download plus enabling the package and the dependencies it downloaded, in one transaction: it clones the Git source (path or URL), selects the numerically newest valid release tag (package-ids), checks it out, verifies the manifest and deploys its package dist. Untagged commits SHALL be ignored; a source without release tags SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

#### Scenario: Install enables pulled dependencies
- **WHEN** `install-catalog app` pulls the missing `lib` from the catalog
- **THEN** both are recorded `enabled: true` and deployed in the same transaction
