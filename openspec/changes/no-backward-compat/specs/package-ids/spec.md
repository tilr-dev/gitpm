## MODIFIED Requirements

### Requirement: Package ID syntax
A package ID SHALL be `namespace/name`. Each component MUST be 1 to 80 ASCII letters, digits, `-` or `_` and MUST NOT start with `-`. Any other ID, an unscoped `name` included, SHALL be refused with `invalid_id`. Verified by `identity::valid_part_accepts_1_to_80_safe_ascii_characters` and `model::valid_id_accepts_scoped_ids_only`.

#### Scenario: Valid scoped ID
- **WHEN** a caller uses the ID `dan/demo1`
- **THEN** the ID is accepted

#### Scenario: Unsafe components are refused
- **WHEN** an ID component is empty, longer than 80 characters, contains `.`, `/`, `\`, a space or non-ASCII text, or starts with `-` (for example `--help`)
- **THEN** the operation fails with `invalid_id` before any file or git command is touched

#### Scenario: Empty namespace
- **WHEN** a caller qualifies an empty namespace with a name
- **THEN** the call fails with `invalid_id`

#### Scenario: Legacy ID equal to an installed namespace
- **WHEN** `dan/x` is installed and the user installs a repository whose manifest ID is the unscoped `dan`
- **THEN** the install fails with `invalid_id`, because unscoped IDs are refused, and the workspace is unchanged

#### Scenario: The word all
- **WHEN** the user runs `gitpm update all`
- **THEN** it is always the joint upgrade (updates), because no package ID can be the single word `all`

### Requirement: Release tag names
Single source for tag names: the release tag of a package SHALL be `namespace/name@VERSION`, both for tags gitpm creates (`init`, `commit`, `fork --replace`) and for tags it reads, in single- and multi-package repositories. Any other tag, `vVERSION` included, SHALL NOT be a release tag. A tag SHALL count only when the manifest at its commit has that ID and version. Verified by `identity::release_tag_is_id_at_version`.

#### Scenario: Tag disagrees with the manifest
- **WHEN** `acme/hello` at version 1.0.0 gets the newest tag `acme/hello@8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag acme/hello@8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog-update` fails with `version_conflict` and writes nothing

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

#### Scenario: Tags that are not release versions
- **WHEN** a repository has tags `dan/demo1@2.0.0-beta`, `dan/demo1@2.0.0+b1` and `dan/demo1@latest` besides valid releases
- **THEN** those tags are not releases: install, update and `catalog-update` never select them and do not warn about them

### Requirement: No default namespace
gitpm SHALL NOT derive or default a namespace: every command that creates a package (`init`) MUST receive a full `NAMESPACE/NAME`, and `fork` MUST receive the target namespace explicitly. There SHALL be no namespace configuration file or command.

#### Scenario: init without a namespace
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** the command fails with `package id needs a namespace: use NAMESPACE/NAME, for example acme/hello` (a CLI usage error: no `Error kind` line, exit status 1) and creates nothing
