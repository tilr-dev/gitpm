## MODIFIED Requirements

### Requirement: Release tag names
All packages in a repository SHALL share one version and tag `vVERSION` (including pre-releases as in "Version format"). Writers SHALL use `git tag -m MESSAGE`, honoring `tag.gpgSign` without an editor. Other forms, including legacy `ID@VERSION`, SHALL be ignored silently. A tag SHALL release only packages listed at its commit with matching manifest ID and version.

#### Scenario: Tag disagrees with the manifest
- **WHEN** `acme/hello` at version 1.0.0 gets the newest tag `v8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag v8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog update` fails with `version_conflict` and writes nothing

#### Scenario: Two tags for one version
- **WHEN** `v1.0.0` and `dan/demo1@1.0.0` point to different commits
- **THEN** only `v1.0.0` is a release of version 1.0.0 and discovery reports no conflict

#### Scenario: Scoped tag
- **WHEN** `commit` releases a single- or multi-package repository at `1.2.3`
- **THEN** it creates only the release tag `v1.2.3`, shared by all its packages

#### Scenario: Legacy tag
- **WHEN** `commit` releases `dan/demo1` at `1.2.3`
- **THEN** it creates `v1.2.3` and never a legacy `dan/demo1@1.2.3` tag

#### Scenario: vVERSION tag of a scoped single-package repository
- **WHEN** a single-package repository whose manifest ID is `dan/demo1` has only the tag `v1.0.0` on a commit with version `1.0.0`
- **THEN** `install` installs `dan/demo1` version 1.0.0 from that repository release

#### Scenario: Legacy tags alone
- **WHEN** a repository has only the tag `dan/demo1@1.0.0` on a commit with version `1.0.0`
- **THEN** `install` fails with `no_releases` and the workspace is unchanged

#### Scenario: Pre-release tag
- **WHEN** a repository has the tag `v2.0.0-beta.1` on a commit whose manifests have version `2.0.0-beta.1`
- **THEN** it is a release tag of version `2.0.0-beta.1`; install, update and `update all` pick it only when asked for (updates, "Pre-releases are opt-in"), and `catalog update` publishes it

#### Scenario: Tags that are not release versions
- **WHEN** a repository has tags `v2.0.0+b1`, `vlatest` and `dan/demo1@2.0.0` besides valid releases
- **THEN** those tags are not releases: install, update and `catalog update` never select them and do not warn about them

#### Scenario: Repository tag does not contain this package
- **WHEN** a `vVERSION` tag points to a commit without the requested package ID, including a commit with another manifest ID
- **THEN** it is not a release of this package and is ignored without warnings, even when newer than all valid releases
- **AND** a version mismatch for a present package with the requested ID remains a broken release tag

### Requirement: Broken historical tags are skipped
Single source for broken release tags: a broken historical release tag (invalid manifest or version mismatch for the requested package ID) SHALL be skipped, not fail discovery; install, `update ID` and `update all` SHALL return it (`InstallResult.skipped`, `RevisionChange.skipped`, `UpdateAllPlan.skipped`). A skipped tag newer than every valid release SHALL still fail selection ("Bad newest tag"). Verified by `releases::bad_historical_tag_does_not_hide_valid_releases`.

#### Scenario: Bad newest tag
- **WHEN** the newest tag is broken and an older one is valid
- **THEN** install, update and `catalog update` fail with `version_conflict` ("latest release tag is broken: …")

#### Scenario: One bad old tag
- **WHEN** tag v0.1.0's manifest is invalid but 0.2.0 is valid
- **THEN** `catalog update` publishes 0.2.0 and warns about 0.1.0
- **AND** `catalog update` warns `Warning: SOURCE: reason`; this is covered by `catalog::update_sources_skips_and_reports_a_broken_historical_tag`

#### Scenario: Update past a broken old tag
- **WHEN** `acme/greeter` 0.1.0 is installed, the origin adds a tag `v0.1.5` whose manifest is invalid and a valid `v0.2.0`, and the user runs `update acme/greeter` or `update all`
- **THEN** the package moves to 0.2.0, the result's `skipped` names `v0.1.5` with its reason, and the CLI prints `Warning: skipped release tag v0.1.5: reason`, as install does

### Requirement: Tags are not policed; integrity via protected catalog
Single source for moved and deleted tags: fetching into any clone (own or shared) SHALL take the origin's release tags as they are: no install, update, `update all` or `catalog update` SHALL fail because a tag moved or was deleted. Installed revisions are kept by commit; the only tag refusal left is a broken newest tag (scenario below). Integrity of published versions is the job of a verified catalog. Verified by `workspace::moved_and_deleted_tags_are_taken_as_they_are`.

#### Scenario: Retagged release
- **WHEN** the repository tag `v0.2.0` of an already seen release is moved to another commit on the origin
- **THEN** `update acme/greeter` and `update all` do not fail because of it; what they report is specified in updates ("Stable update never downgrades")

#### Scenario: Moved tag of a sibling
- **WHEN** the shared repository tag `v0.1.0` of installed `tilr/theme` moved on the origin and the user updates `tilr/store` from the same shared clone
- **THEN** the update of `tilr/store` succeeds and `tilr/theme` keeps its installed commit
- **AND** this is covered by `a_moved_tag_of_an_installed_package_does_not_block_a_sibling`

#### Scenario: Moved onto a commit of another version
- **WHEN** the installed release tag `v0.2.0` is moved onto a commit whose manifest says 0.3.0
- **THEN** it is a broken tag: `update all` keeps the package with the warning `release tag v0.2.0 now points to COMMIT (skipped: REASON); the installed commit is kept`, while `update acme/greeter` fails with `version_conflict` ("latest release tag is broken: …") because it is the newest tag

#### Scenario: Moved tag on regenerate
- **WHEN** a published tag was moved to another commit and the user runs `catalog update`
- **THEN** the published record is kept and a warning names the moved tag (catalog, "Published versions are immutable")
