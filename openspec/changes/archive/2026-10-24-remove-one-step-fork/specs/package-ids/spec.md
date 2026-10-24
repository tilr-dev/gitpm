## MODIFIED Requirements

### Requirement: Release tag names
The release tag of a package SHALL be `namespace/name@VERSION` with VERSION as in "Version format" (a pre-release included), for tags gitpm creates (`init`, `commit`) and for tags it reads, in single- and multi-package repositories. Any other tag, `vVERSION` and versions with build metadata included, SHALL NOT be a release tag. A tag SHALL count only when the manifest at its commit has that ID and version. Verified by `identity::release_tag_is_id_at_version`.

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
