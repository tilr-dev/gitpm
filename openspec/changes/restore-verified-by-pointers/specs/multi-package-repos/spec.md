## MODIFIED Requirements

### Requirement: Package validity is per package
Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it.

Verified by `workspace::a_defective_sibling_does_not_block_a_package`.

#### Scenario: Broken sibling
- **WHEN** `packages/theme` has no `dist/` and the user installs `tilr/store` from the same repository
- **THEN** the install succeeds

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. The clone SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

Verified by `two_packages_share_one_clone_and_follow_repository_releases`.

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone with the usual local-work guards and refuses without changes if it holds local work
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

## ADDED Requirements

### Requirement: Package structure and repository uniqueness
Every package SHALL need a `namespace/name` ID unique in the repository in any letter case (package-ids, "Case-colliding IDs are refused"), a manifest and a committed package output folder selected by its manifest dist (default dist/) (neither a symlink; the missing package dist message is in dist), and packages MUST NOT nest.

Verified by `packages::invalid_package_sets_are_refused` and `packages::ids_differing_only_in_case_make_both_folders_defective`.

#### Scenario: Case variants
- **WHEN** two packages of the repository have IDs differing only in letter case
- **THEN** both folders are defective
- **AND** this is covered by `packages::ids_differing_only_in_case_make_both_folders_defective`
