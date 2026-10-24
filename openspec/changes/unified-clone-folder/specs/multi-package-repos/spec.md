## MODIFIED Requirements

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share one clone `repos/<name>-<hash of the source>`; state SHALL record a source-derived `clone` key for every Git package and `path` for a nested package (absent for a root package) and only the committed manifest dist folder relative to `<path>` (default `<path>/dist`) SHALL be deployed. Single source for removing a shared clone: it SHALL be removed with the last package (recorded or forgotten) that uses it, with its lost work guarded then. Fetching into it takes the origin's tags as they are (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

### Requirement: Root package that moved into a folder
A root package SHALL keep its source-keyed `repos/<key>` clone when a later release moves it into a package folder (`path` set, `clone` unchanged) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store` and preserves its `clone` key, so other packages from the same source reuse the clone
