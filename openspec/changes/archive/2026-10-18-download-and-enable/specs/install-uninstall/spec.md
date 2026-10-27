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
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the numerically newest valid release tag (see package-ids), check it out detached, verify the manifest and deploy its package dist; missing dependencies are installed as in dependencies. An install SHALL be a download plus enabling the package and the dependencies it downloaded, in one transaction; `install --force` (`Manager::install_package_with`, `install_from_catalog_with` with `force`; also `install-catalog --force`) SHALL enable the named package even when a dependency is not active (dependencies, "enable needs active dependencies"), while missing dependencies are installed as usual. Untagged commits SHALL be ignored; a source without release tags SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions` and `forced_install_and_install_over_a_download`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

#### Scenario: Install enables pulled dependencies
- **WHEN** `install-catalog app` pulls the missing `lib` from the catalog
- **THEN** both are recorded `enabled: true` and deployed in the same transaction

#### Scenario: Forced install next to a disabled dependency
- **WHEN** `lib` is installed but disabled and the user runs `install ./app --force` (or `install-catalog app --force`), where `app` requires `lib ^1`
- **THEN** `app` is recorded enabled and deployed, `lib` stays disabled, and `list` reports `app`'s dependency on `lib` unsatisfied

### Requirement: Install is idempotent for the same source
Installing an installed and enabled package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)` (with a mode: `(VERSION, pinned at REF)`), with `; run update` when the source resolves to another revision or mode (`; requested --ref REF — run update ID --ref REF`). Installing a package recorded disabled (for example by `download`) from the same source SHALL enable it at its recorded revision and mode with the check of dependencies, "enable needs active dependencies" (`--force` bypasses it), and report it as an install (`installed ID@V from SOURCE`, `enabled` true, `already_installed` false, `available` and `requested_mode` as for an installed package, and a warning when the requested mode differs from the recorded one). "Same source" SHALL mean the same string as recorded after canonicalizing a local path. `InstallResult` SHALL carry `already_installed`, `available`, the recorded `mode`, `requested_mode` and `enabled`.

#### Scenario: Repeat install
- **WHEN** `acme/greeter` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `acme/greeter already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `acme/greeter already installed (0.2.0); run update` and changes nothing

#### Scenario: Install over a download
- **WHEN** `app` was downloaded from `./app` and the user runs `install ./app`
- **THEN** it fails with `dependency_conflict` while a dependency is not active; with every dependency active, or with `--force`, `app` is enabled and deployed and the line is `installed app@V from SOURCE`; verified by `forced_install_and_install_over_a_download`
