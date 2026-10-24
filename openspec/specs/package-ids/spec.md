# package-ids Specification

## Purpose

Defines how packages are named and versioned: the `namespace/name` ID (unscoped IDs are refused), release tag names, how releases are discovered from tags (broken and moved tags), and the MAJOR.MINOR.PATCH version format shared by manifests, tags and catalogs.

## Requirements

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

### Requirement: No default namespace
gitpm SHALL NOT derive or default a namespace: every operation that creates a package (`init`) MUST receive a full `NAMESPACE/NAME`, and `fork` MUST receive the target namespace explicitly. The library SHALL refuse a name without a namespace with `invalid_id`. There SHALL be no namespace configuration file or command.

#### Scenario: init without a namespace
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** the library fails with `invalid_id` ("package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello"), the CLI prints `Error kind: invalid_id`, the exit status is 1 and nothing is created

### Requirement: Release tag names
The release tag of a repository SHALL be `vVERSION` with VERSION as in "Version format" (a pre-release included), for tags gitpm creates (`init`, `commit`) and for tags it reads, in single- and multi-package repositories.

#### Scenario: Release tag names contract
- **WHEN** gitpm reads or writes repository release tags
- **THEN** All packages in one repository SHALL share one version and one release tag. Every release-tag writer SHALL create the single tag using `git tag -m MESSAGE`, honoring `tag.gpgSign` without opening an editor. Legacy `ID@VERSION` tags, unprefixed version tags, tags with build metadata and all other tag forms SHALL NOT be release tags and SHALL be ignored without a warning. A tag SHALL count as a release of a package only when the package is listed at its commit and its manifest has that ID and version.

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

### Requirement: Version format
Every package version (manifest, `forkOf`, `replaces`, `--version`, catalog, release tag) MUST be SemVer `MAJOR.MINOR.PATCH` without leading zeros, with an optional pre-release `-PRE` (SemVer identifiers); anything else, build metadata included, SHALL be refused with `invalid_version` (an empty manifest version: `invalid_manifest`). Versions SHALL be ordered by SemVer precedence. Verified by `catalog::version_key_follows_semver_precedence`, `catalog::version_key_rejects_everything_else`.

#### Scenario: Numeric ordering
- **WHEN** releases `1.9.0` and `1.10.0` exist
- **THEN** `1.10.0` is the newer release

#### Scenario: Malformed version is refused
- **WHEN** a manifest declares version `01.0.0`, `1.0`, a malformed pre-release suffix `1.0.0-01` or the build suffix `1.0.0+build.5`
- **THEN** reading it fails with `invalid_version` (an empty `version` fails with `invalid_manifest`), while the pre-release `1.0.0-beta.1` is accepted

#### Scenario: Pre-release ordering
- **WHEN** `1.0.0-alpha`, `1.0.0-beta`, `1.0.0-beta.2` and `1.0.0` exist
- **THEN** they are ordered `1.0.0-alpha < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0`

### Requirement: IDs are matched exactly
Operations on an installed package SHALL match the ID exactly as recorded in `state.json`; an ID that differs only in letter case MUST NOT resolve to the installed package, even on a case-insensitive file system. Verified by `uninstall::case_variant_id_never_touches_the_installed_plugin`.

#### Scenario: Case variant on APFS
- **WHEN** `vendor/module` is installed and the user runs `uninstall Vendor/Module`
- **THEN** the command fails with `repository_not_found` and `vendor/module` is untouched

### Requirement: Case-colliding IDs are refused
Single source for IDs that differ only in letter case, refused independent of the file system's case sensitivity: installing or restoring a package whose ID differs only in case from an installed or retained package, or from an existing workspace path, SHALL fail with `workspace`; `fork` and `init` into a multi-package repository SHALL fail with `invalid_argument`. Verified by `workspace::ids_differing_only_in_case_are_refused`.

#### Scenario: Case variants inside one repository
- **WHEN** a multi-package repository lists two packages whose IDs differ only in letter case
- **THEN** both folders are defective (multi-package-repos, "Package validity is per package")

#### Scenario: Install after forget of a case variant
- **WHEN** `dan/p` is retained in the workspace and the user installs a repository whose manifest ID is `Dan/p`
- **THEN** the install fails with `workspace` and the workspace is unchanged
- **AND** the message says `… differs only in letter case from installed …` (or, for a colliding workspace path, `… collides with an existing path that differs only in letter case`)

#### Scenario: ID differing only in case
- **WHEN** the repository has `a/ui` and the user runs `init A/ui --into REPO`
- **THEN** init fails with `invalid_argument` (`package a/ui already exists in this repository`) and the repository is unchanged

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
