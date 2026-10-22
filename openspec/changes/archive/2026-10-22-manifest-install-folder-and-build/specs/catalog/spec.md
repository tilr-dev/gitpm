## MODIFIED Requirements

### Requirement: Release record
Single source for the release record: a release SHALL record the whole `manifest`, `source`, `commit`, `reference`, `files` (SHA-256 and size of every file of the committed output folder selected by manifest dist, default dist/), optional `build` `{url, sha256}` (format only; installs come from Git) and, for a package of a multi-package repository, `path`. Every release MUST have file hashes.

#### Scenario: Legacy release without hashes
- **WHEN** a catalog holds an unscoped release of the v1 era without `files`
- **THEN** reading it fails with `catalog`; no release may omit file hashes

#### Scenario: Namespaced release without hashes
- **WHEN** a namespaced release record has an empty `files` while its commit has `dist/index.js`
- **THEN** the release is refused with `catalog` instead of skipping verification

#### Scenario: Declared folder hashes
- **WHEN** a package selects its root or a custom output folder
- **THEN** catalog generation and verification hash exactly the same committed files as deployment
