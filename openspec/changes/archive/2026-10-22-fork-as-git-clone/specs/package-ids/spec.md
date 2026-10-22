## MODIFIED Requirements

### Requirement: Case-colliding IDs are refused
Single source for IDs that differ only in letter case, refused independent of the file system's case sensitivity: installing or restoring a package whose ID differs only in case from an installed or retained package, or from an existing workspace path, SHALL fail with `workspace`; `fork` and `init` into a multi-package repository SHALL fail with `invalid_argument`. Verified by `workspace::ids_differing_only_in_case_are_refused`.

#### Scenario: Case variants inside one repository
- **WHEN** a multi-package repository lists two packages whose IDs differ only in letter case
- **THEN** both folders are defective (multi-package-repos, "Package validity is per package")

#### Scenario: Install after forget of a case variant
- **WHEN** `dan/p` is retained in the workspace and the user installs a repository whose manifest ID is `Dan/p`
- **THEN** the install fails with `workspace` and the workspace is unchanged
- **AND** the message says `… differs only in letter case from installed …` (or, for a colliding workspace path, `… collides with an existing path that differs only in letter case`)

#### Scenario: ID differing only in case
- **WHEN** the repository has `a/ui` and the user runs `init A/ui --into REPO`
- **THEN** init fails with `invalid_argument` (`package a/ui already exists in this repository`) and the repository is unchanged
