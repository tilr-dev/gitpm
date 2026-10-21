## MODIFIED Requirements

### Requirement: Catalog generated from Git release tags
`catalog update [--sources FILE]` SHALL read the repositories listed in the sources file (one per line, relative to the file; blank and `#` lines skipped), discover every release tag, pin each tag's commit, hash its package dist and write the tree. For a multi-package repository it SHALL publish only the packages listed in `gitpm.json` at HEAD. Broken historical tags are skipped as in package-ids. It SHALL validate every source before writing anything and retain releases of removed sources.

#### Scenario: Empty sources file
- **WHEN** the sources file has only comments and blank lines
- **THEN** `catalog update` fails with `invalid_argument` ("sources file has no repositories")

#### Scenario: Two releases
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0
- **THEN** `catalog update` prints `Catalog ready: 2 release(s) checked from sources.`
- **AND** this is covered by `catalog_update_keeps_published_records_when_tags_move` and `catalog_update_validates_every_source_before_writing_and_retains_removed_sources`

#### Scenario: Package removed from gitpm.json at HEAD
- **WHEN** a multi-package repository tagged `tilr/old@0.1.0` but no longer lists `packages/old` in `gitpm.json` at HEAD, and the catalog has no `tilr/old` yet
- **THEN** `catalog update` publishes the releases of the packages listed at HEAD and no release of `tilr/old`

#### Scenario: Duplicate source lines
- **WHEN** the sources file lists the same repository twice, also in another spelling of its path
- **THEN** `catalog update` reads it once and indexes each release once


#### Scenario: Submodule inside dist
- **WHEN** the release contains a Git submodule at `dist/vendor`
- **THEN** publication succeeds, hashing the staged export with an empty `vendor/` directory
- **AND** the author is warned that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

### Requirement: Register one version
`catalog register SOURCE --ref REF [--package ID] [--build-url HTTPS_URL --build-sha256 SHA256]` SHALL add one committed version to a local catalog and print the release as JSON; build options MUST be given together (a CLI usage error otherwise) and the build URL MUST be HTTPS with a valid SHA-256 (`catalog` otherwise). Verified by `local_catalog_register_read_install_and_immutable_versions` and `invalid_build_metadata_does_not_publish_release`.

#### Scenario: Invalid build metadata
- **WHEN** `--build-url` is `http://…` or the SHA-256 is malformed
- **THEN** catalog register fails with `catalog` ("build URL must use HTTPS" / "invalid build SHA-256") and no release is published (an empty top-level `catalog.json` may already have been created)


#### Scenario: Submodule inside dist
- **WHEN** the release contains a Git submodule at `dist/vendor`
- **THEN** publication succeeds, hashing the staged export with an empty `vendor/` directory
- **AND** the author is warned that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule
