## MODIFIED Requirements

### Requirement: Per-package releases
Releases SHALL be per package with tags `ID@VERSION` (package-ids, "Release tag names"), and the package set SHALL be read from `gitpm.json` at the tagged commit so a package may move between releases. A tag whose commit lacks its package SHALL be skipped as a broken tag (package-ids). Other tags, `vVERSION` included, are not releases and SHALL be ignored without a warning. Verified by `a_package_can_move_to_another_folder_between_releases`.

#### Scenario: Newest tag lacks its package
- **WHEN** the newest tag `tilr/store@0.3.0` points to a commit without `tilr/store`
- **THEN** it is skipped ("tag NAME points to commit … without package ID") and, being the newest, makes selecting the latest release fail with `version_conflict` as package-ids specifies; an older such tag only produces the warning
- **AND** this is covered by `a_tag_on_a_commit_without_its_package_is_a_clear_error`

#### Scenario: Independent releases
- **WHEN** `tilr/store@0.2.0` is tagged while `tilr/theme` stays at 0.1.0
- **THEN** `update tilr/store` moves it to 0.2.0 and `tilr/theme` is untouched

#### Scenario: vVERSION tag in a multi-package repository
- **WHEN** a multi-package repository also has the tag `v0.2.0`
- **THEN** it is no release of any package and install, update and `catalog-update` print no warning about it
