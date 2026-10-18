## MODIFIED Requirements

### Requirement: One shared clone per source
Packages of one source SHALL share one clone `repos/<name>-<hash of the source>`; state SHALL record `clone` and `path` per package and only `<path>/dist` SHALL be deployed. Single source for removing a shared clone: it SHALL be removed with the last package (recorded or forgotten) that uses it, with its lost work guarded then. Fetching into it takes the origin's tags as they are (package-ids). Verified by `two_packages_of_one_repository_share_one_clone_and_update_independently`.

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

