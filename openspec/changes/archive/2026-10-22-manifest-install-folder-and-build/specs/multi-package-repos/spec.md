## MODIFIED Requirements

### Requirement: Package validity is per package
Every package SHALL need a `namespace/name` ID unique in the repository in any letter case (package-ids, "Case-colliding IDs are refused"), a manifest and a committed package output folder selected by its manifest dist (default dist/) (neither a symlink; the missing package dist message is in dist), and packages MUST NOT nest. Only the package being installed, updated or registered must be valid at its commit; a defective sibling SHALL NOT block it.

#### Scenario: Case variants
- **WHEN** two packages of the repository have IDs differing only in letter case
- **THEN** both folders are defective
- **AND** this is covered by `packages::ids_differing_only_in_case_make_both_folders_defective`

#### Scenario: Broken sibling
- **WHEN** `packages/theme` has no `dist/` and the user installs `tilr/store` from the same repository
- **THEN** the install succeeds

### Requirement: One shared clone per source
Packages of one source SHALL share one clone `repos/<name>-<hash of the source>`; state SHALL record `clone` and `path` per package and only the committed manifest dist folder relative to `<path>` (default `<path>/dist`) SHALL be deployed. Single source for removing a shared clone: it SHALL be removed with the last package (recorded or forgotten) that uses it, with its lost work guarded then. Fetching into it takes the origin's tags as they are (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`
