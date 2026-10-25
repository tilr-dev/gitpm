## MODIFIED Requirements

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (every output file’s SHA-256 and size), optional `build` `{url, sha256}` and, for a multi-package repository’s package, `path`. Every release MUST have file hashes.

Verified by `catalog::validate_release_checks_identity_commit_paths_and_build` and `workspace::release_with_empty_files_is_still_verified`.

#### Scenario: Legacy release without hashes
- **WHEN** a catalog holds an unscoped release of the v1 era without `files`
- **THEN** reading it fails with `catalog`; no release may omit file hashes

#### Scenario: Namespaced release without hashes
- **WHEN** a namespaced release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

## ADDED Requirements

### Requirement: Release output selection
Build SHALL be preferred over Git; its output is the archive root, while Git output is the committed manifest-selected folder (default dist/).

Verified by `declared_output_folders_install_redeploy_and_verify_catalog` and `install_update_and_offline_rollback_need_no_repository`.

#### Scenario: Declared folder hashes
- **WHEN** a Git release selects its root or a custom output folder
- **THEN** catalog generation and verification hash exactly the same committed files as deployment
