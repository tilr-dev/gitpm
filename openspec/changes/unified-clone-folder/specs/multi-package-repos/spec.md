## MODIFIED Requirements

### Requirement: One shared clone per source
Root and nested Git packages of one source SHALL share `repos/<name>-<hash of the source>`. State SHALL record `clone` and, for nested packages, `path`; only the committed manifest output relative to that path SHALL be deployed. The clone SHALL be removed only with its last installed or forgotten user, guarding lost work then. Fetching mirrors origin tags (package-ids).

#### Scenario: Uninstall one of two
- **WHEN** `tilr/store` and `tilr/theme` share a clone and `tilr/store` is uninstalled
- **THEN** the clone stays and `tilr/theme` keeps working
- **AND** this is covered by `uninstalling_one_package_keeps_the_other_working`

#### Scenario: Clone-free build switches to Git beside a sibling
- **WHEN** a clone-free build switches to a Git ref and an installed or forgotten sibling already uses the same source-keyed clone
- **THEN** the transition reuses that clone with the usual local-work guards and refuses without changes if it holds local work
- **AND** a successful transition records the shared clone key and preserves the sibling and its deployed output

### Requirement: Root package that moved into a folder
A root package SHALL keep its source-keyed `repos/<key>` clone when a later release moves it into a package folder (`path` set, `clone` unchanged) and SHALL keep updating and forking. Verified by `a_root_package_moved_into_a_folder_keeps_updating_and_forks`.

#### Scenario: Move into packages/
- **WHEN** an installed root package's next release lives in `packages/store` under a new `gitpm.json`
- **THEN** `update` succeeds and state records `path` `packages/store` and preserves its `clone` key, so other packages from the same source reuse the clone
