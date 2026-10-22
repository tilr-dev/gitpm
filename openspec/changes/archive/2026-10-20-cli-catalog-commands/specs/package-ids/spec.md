## MODIFIED Requirements

### Requirement: Release tag names
The release tag of a package SHALL be `namespace/name@VERSION` with VERSION as in "Version format" (a pre-release included), for tags gitpm creates (`init`, `commit`, `fork --replace`) and for tags it reads, in single- and multi-package repositories. Any other tag, `vVERSION` and versions with build metadata included, SHALL NOT be a release tag. A tag SHALL count only when the manifest at its commit has that ID and version. Verified by `identity::release_tag_is_id_at_version`.

#### Scenario: Tag disagrees with the manifest
- **WHEN** `acme/hello` at version 1.0.0 gets the newest tag `acme/hello@8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag acme/hello@8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog update` fails with `version_conflict` and writes nothing

#### Scenario: Two tags for one version
- **WHEN** `v1.0.0` and `dan/demo1@1.0.0` point to different commits
- **THEN** only `dan/demo1@1.0.0` is a release of version 1.0.0 and discovery reports no conflict

#### Scenario: Scoped tag
- **WHEN** `dan/demo1` is released at `1.2.3`
- **THEN** the release tag is `dan/demo1@1.2.3`

#### Scenario: Legacy tag
- **WHEN** `commit` releases `dan/demo1` at `1.2.3`
- **THEN** it creates `dan/demo1@1.2.3` and never a `v1.2.3` tag

#### Scenario: vVERSION tag of a scoped single-package repository
- **WHEN** a single-package repository whose manifest ID is `dan/demo1` has only the tag `v1.0.0` on a commit with version `1.0.0`
- **THEN** `install` fails with `no_releases` and the workspace is unchanged

#### Scenario: Pre-release tag
- **WHEN** a repository has the tag `dan/demo1@2.0.0-beta.1` on a commit whose manifest has version `2.0.0-beta.1`
- **THEN** it is a release tag of version `2.0.0-beta.1`; install, update and `update all` pick it only when asked for (updates, "Pre-releases are opt-in"), and `catalog update` publishes it

#### Scenario: Tags that are not release versions
- **WHEN** a repository has tags `dan/demo1@2.0.0+b1` and `dan/demo1@latest` besides valid releases
- **THEN** those tags are not releases: install, update and `catalog update` never select them and do not warn about them

### Requirement: Broken historical tags are skipped
Single source for broken release tags: a broken historical release tag (invalid manifest, manifest mismatch, package missing at the commit) SHALL be skipped, not fail discovery; install, `update ID` and `update all` SHALL return it (`InstallResult.skipped`, `RevisionChange.skipped`, `UpdateAllPlan.skipped`). A skipped tag newer than every valid release SHALL still fail selection ("Bad newest tag"). Verified by `releases::bad_historical_tag_does_not_hide_valid_releases`.

#### Scenario: Bad newest tag
- **WHEN** the newest tag is broken and an older one is valid
- **THEN** install, update and `catalog update` fail with `version_conflict` ("latest release tag is broken: …")

#### Scenario: One bad old tag
- **WHEN** tag 0.1.0's manifest is invalid but 0.2.0 is valid
- **THEN** `catalog update` publishes 0.2.0 and warns about 0.1.0
- **AND** `catalog update` warns `Warning: SOURCE: reason`; this is covered by `catalog::update_sources_skips_and_reports_a_broken_historical_tag`

#### Scenario: Update past a broken old tag
- **WHEN** `acme/greeter` 0.1.0 is installed, the origin adds a tag `acme/greeter@0.1.5` whose manifest is invalid and a valid `acme/greeter@0.2.0`, and the user runs `update acme/greeter` or `update all`
- **THEN** the package moves to 0.2.0, the result's `skipped` names `acme/greeter@0.1.5` with its reason, and the CLI prints `Warning: skipped release tag acme/greeter@0.1.5: reason`, as install does

### Requirement: Tags are not policed; integrity via protected catalog
Single source for moved and deleted tags: fetching into any clone (own or shared) SHALL take the origin's release tags as they are: no install, update, `update all` or `catalog update` SHALL fail because a tag moved or was deleted. Installed revisions are kept by commit; the only tag refusal left is a broken newest tag (scenario below). Integrity of published versions is the job of a verified catalog. Verified by `workspace::moved_and_deleted_tags_are_taken_as_they_are`.

#### Scenario: Retagged release
- **WHEN** the tag of an already seen release `acme/greeter@0.2.0` is moved to another commit on the origin
- **THEN** `update acme/greeter` and `update all` do not fail because of it; what they report is specified in updates ("Stable update never downgrades")

#### Scenario: Moved tag of a sibling
- **WHEN** the tag of installed `tilr/theme@0.1.0` moved on the origin and the user updates `tilr/store` from the same shared clone
- **THEN** the update of `tilr/store` succeeds and `tilr/theme` keeps its installed commit
- **AND** this is covered by `a_moved_tag_of_an_installed_package_does_not_block_a_sibling`

#### Scenario: Moved onto a commit of another version
- **WHEN** the installed release tag `acme/greeter@0.2.0` is moved onto a commit whose manifest says 0.3.0
- **THEN** it is a broken tag: `update all` keeps the package with the warning `release tag acme/greeter@0.2.0 now points to COMMIT (skipped: REASON); the installed commit is kept`, while `update acme/greeter` fails with `version_conflict` ("latest release tag is broken: …") because it is the newest tag

#### Scenario: Moved tag on regenerate
- **WHEN** a published tag was moved to another commit and the user runs `catalog update`
- **THEN** the published record is kept and a warning names the moved tag (catalog, "Published versions are immutable")
