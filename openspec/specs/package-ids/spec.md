# package-ids Specification

## Purpose

Defines how packages are named and versioned: the `namespace/name` ID (unscoped IDs are refused), release tag names, how releases are discovered from tags (broken and moved tags), and the MAJOR.MINOR.PATCH version format shared by manifests, tags and catalogs.

## Requirements

### Requirement: Package ID syntax
A package ID SHALL be an immutable UUID in lowercase hyphenated form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`. Package references in manifests, state, dependencies, replacements, CLI and FFI SHALL contain only this UUID, without a package name hint. A package name SHALL be a nonunique alias, never an identifier or a lookup key. Invalid IDs, including `namespace/name` and bare names, SHALL fail with `invalid_id` before any mutation.

#### Scenario: UUID accepted
- **WHEN** a caller uses `11111111-1111-4111-8111-111111111111`
- **THEN** it is accepted as a package ID

#### Scenario: Scoped and bare IDs refused
- **WHEN** a caller uses `acme/hello`, `hello`, `all`, or a malformed UUID as a package ID
- **THEN** it fails with `invalid_id` and creates nothing

#### Scenario: Names do not resolve
- **WHEN** two installed packages have the same name and the caller supplies that name to a package operation
- **THEN** the operation refuses it as an invalid ID rather than choosing either package

#### Scenario: The word all
- **WHEN** the user runs `gitpm update all`
- **THEN** it performs the joint upgrade

#### Scenario: Valid scoped ID
- **WHEN** a caller supplies the formerly valid scoped ID `dan/demo1`
- **THEN** it fails with `invalid_id`; a scoped spelling is never interpreted as a UUID

#### Scenario: Unsafe components are refused
- **WHEN** a package reference contains a path separator, traversal component, whitespace or nonhex text instead of a canonical UUID
- **THEN** it fails with `invalid_id` before any filesystem or Git mutation

#### Scenario: Empty namespace
- **WHEN** a package reference uses `/hello` with an empty legacy namespace component
- **THEN** it fails with `invalid_id` and creates nothing

#### Scenario: Legacy ID equal to an installed namespace
- **WHEN** a package with a UUID and name `dan` is installed and a caller supplies the bare reference `dan`
- **THEN** it fails with `invalid_id` and leaves the installed package unchanged

### Requirement: Release tag names
All packages in a repository SHALL share one version and tag `vVERSION` (including pre-releases as in "Version format"). Writers SHALL use `git tag -m MESSAGE`, honoring `tag.gpgSign` without an editor. Other forms, including legacy `ID@VERSION`, SHALL be ignored silently. A tag SHALL release only packages listed at its commit with matching manifest ID and version.

#### Scenario: Tag disagrees with the manifest
- **WHEN** `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` at version 1.0.0 gets the newest tag `v8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag v8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog update` fails with `version_conflict` and writes nothing

#### Scenario: Two tags for one version
- **WHEN** `v1.0.0` and `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc@1.0.0` point to different commits
- **THEN** only `v1.0.0` is a release of version 1.0.0 and discovery reports no conflict

#### Scenario: Scoped tag
- **WHEN** `commit` releases a single- or multi-package repository at `1.2.3`
- **THEN** it creates only the release tag `v1.2.3`, shared by all its packages

#### Scenario: Legacy tag
- **WHEN** `commit` releases `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc` at `1.2.3`
- **THEN** it creates `v1.2.3` and never a legacy `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc@1.2.3` tag

#### Scenario: vVERSION tag of a scoped single-package repository
- **WHEN** a single-package repository whose manifest ID is `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc` has only the tag `v1.0.0` on a commit with version `1.0.0`
- **THEN** `install` installs `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc` version 1.0.0 from that repository release

#### Scenario: Legacy tags alone
- **WHEN** a repository has only the tag `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc@1.0.0` on a commit with version `1.0.0`
- **THEN** `install` fails with `no_releases` and the workspace is unchanged

#### Scenario: Pre-release tag
- **WHEN** a repository has the tag `v2.0.0-beta.1` on a commit whose manifests have version `2.0.0-beta.1`
- **THEN** it is a release tag of version `2.0.0-beta.1`; install, update and `update all` pick it only when asked for (updates, "Pre-releases are opt-in"), and `catalog update` publishes it

#### Scenario: Tags that are not release versions
- **WHEN** a repository has tags `v2.0.0+b1`, `vlatest` and `ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc@2.0.0` besides valid releases
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
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.1.0 is installed, the origin adds a tag `v0.1.5` whose manifest is invalid and a valid `v0.2.0`, and the user runs `update d135a7cf-1c63-549a-93cb-8f89656b2ced` or `update all`
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
Operations SHALL match the canonical UUID recorded in `state.json`. UUID identity SHALL remain unchanged by a package name change; names SHALL NOT be resolved as IDs.

#### Scenario: Name change keeps identity
- **WHEN** a release changes the name of an installed package while preserving its UUID
- **THEN** update selects it by the same UUID and dependency and replacement references remain valid

#### Scenario: Case variant on APFS
- **WHEN** a caller supplies an uppercase spelling of an installed lowercase UUID on a case-insensitive filesystem
- **THEN** the operation fails with `invalid_id` and leaves the installed package unchanged

### Requirement: Case-colliding IDs are refused
Only canonical lowercase UUIDs SHALL be accepted as package IDs, independent of filesystem case sensitivity. A noncanonical case variant SHALL fail with `invalid_id`. Duplicate UUIDs in one repository SHALL make the affected packages invalid; duplicate names SHALL be allowed.

#### Scenario: Noncanonical UUID
- **WHEN** a caller supplies an uppercase UUID
- **THEN** the operation fails with `invalid_id` and changes nothing

#### Scenario: Equal names
- **WHEN** two packages have the same name and different UUIDs
- **THEN** both can be installed together and have separate state records and deployment folders

#### Scenario: Case variants inside one repository
- **WHEN** one repository contains a canonical lowercase UUID and an uppercase spelling of that UUID
- **THEN** the uppercase manifest is invalid independently of filesystem case sensitivity

#### Scenario: Install after forget of a case variant
- **WHEN** a canonical UUID is retained after forget and another source declares an uppercase spelling of that UUID
- **THEN** installation fails with `invalid_id` and the retained package remains unchanged

#### Scenario: ID differing only in case
- **WHEN** a selector differs from a package's canonical UUID only in letter case
- **THEN** the selector is refused with `invalid_id`, without resolving or modifying the package

### Requirement: Tags are not policed; integrity via protected catalog
Single source for moved and deleted tags: fetching into any clone (own or shared) SHALL take the origin's release tags as they are: no install, update, `update all` or `catalog update` SHALL fail because a tag moved or was deleted. Installed revisions are kept by commit; the only tag refusal left is a broken newest tag (scenario below). Integrity of published versions is the job of a verified catalog. Verified by `workspace::moved_and_deleted_tags_are_taken_as_they_are`.

#### Scenario: Retagged release
- **WHEN** the repository tag `v0.2.0` of an already seen release is moved to another commit on the origin
- **THEN** `update d135a7cf-1c63-549a-93cb-8f89656b2ced` and `update all` do not fail because of it; what they report is specified in updates ("Stable update never downgrades")

#### Scenario: Moved tag of a sibling
- **WHEN** the shared repository tag `v0.1.0` of installed `f851f609-bb81-52d6-bfef-33798624782c` moved on the origin and the user updates `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` from the same shared clone
- **THEN** the update of `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` succeeds and `f851f609-bb81-52d6-bfef-33798624782c` keeps its installed commit
- **AND** this is covered by `a_moved_tag_of_an_installed_package_does_not_block_a_sibling`

#### Scenario: Moved onto a commit of another version
- **WHEN** the installed release tag `v0.2.0` is moved onto a commit whose manifest says 0.3.0
- **THEN** it is a broken tag: `update all` keeps the package with the warning `release tag v0.2.0 now points to COMMIT (skipped: REASON); the installed commit is kept`, while `update d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `version_conflict` ("latest release tag is broken: …") because it is the newest tag

#### Scenario: Moved tag on regenerate
- **WHEN** a published tag was moved to another commit and the user runs `catalog update`
- **THEN** the published record is kept and a warning names the moved tag (catalog, "Published versions are immutable")
