# package-ids Specification

## Purpose

Defines how packages are named and versioned: the `namespace/name` ID, legacy unscoped IDs, release tag names and the MAJOR.MINOR.PATCH version format shared by manifests, tags and catalogs.

## Requirements

### Requirement: Package ID syntax
A package ID SHALL be `namespace/name` or, for legacy packages only, a single unscoped `name`. Each component MUST be 1 to 80 ASCII letters, digits, `-` or `_` and MUST NOT start with `-`. Any other ID SHALL be refused with `invalid_id`. Verified by `identity::valid_part_accepts_1_to_80_safe_ascii_characters` and `model::valid_id_accepts_legacy_and_scoped_ids_only`.

#### Scenario: Valid scoped ID
- **WHEN** a caller uses the ID `dan/demo1`
- **THEN** the ID is accepted

#### Scenario: Unsafe components are refused
- **WHEN** an ID component is empty, longer than 80 characters, contains `.`, `/`, `\`, a space or non-ASCII text, or starts with `-` (for example `--help`)
- **THEN** the operation fails with `invalid_id` before any file or git command is touched

#### Scenario: Empty namespace
- **WHEN** a caller qualifies an empty namespace with a name
- **THEN** the call fails with `invalid_id`

### Requirement: No default namespace
gitpm SHALL NOT derive or default a namespace: every command that creates a package (`init`, `create`) MUST receive a full `NAMESPACE/NAME`, and `fork` MUST receive the target namespace explicitly. There SHALL be no namespace configuration file or command.

#### Scenario: init without a namespace
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** the command fails with `plugin id needs a namespace: use NAMESPACE/NAME, for example acme/hello` and creates nothing

### Requirement: Release tag names
Single source for tag names: tags gitpm creates (`init`, `commit`, `fork --replace`) SHALL be `namespace/name@VERSION` for a scoped package and `vVERSION` for a legacy one. For the root package of a single-package repository, reading releases SHALL accept both `ID@VERSION` and `vVERSION` (multi-package repositories: see multi-package-repos). A tag SHALL count only when the manifest at its commit has that ID and version. Verified by `identity::release_tag_is_v_for_legacy_and_at_for_scoped_ids`.

#### Scenario: Tag disagrees with the manifest
- **WHEN** the legacy package `hello` at version 1.0.0 gets the newest tag `v8.0.0` on a commit whose manifest still says 1.0.0
- **THEN** the tag is skipped ("tag v8.0.0 disagrees with manifest version 1.0.0") and, because it is the newest, `catalog-update` fails and writes nothing; this is covered by `release_manifest_must_match_tag_and_update_rejects_retagging`

#### Scenario: Two tags for one version
- **WHEN** `v1.0.0` and `dan/demo1@1.0.0` point to different commits
- **THEN** release discovery fails with `version_conflict` ("conflicting release tags for dan/demo1@1.0.0")

#### Scenario: Scoped tag
- **WHEN** `dan/demo1` is released at `1.2.3`
- **THEN** the release tag is `dan/demo1@1.2.3`

#### Scenario: Legacy tag
- **WHEN** the legacy package `demo1` is released at `1.2.3`
- **THEN** the release tag is `v1.2.3`

#### Scenario: vVERSION tag of a scoped single-package repository
- **WHEN** a single-package repository whose manifest ID is `dan/demo1` has only the tag `v1.0.0` on a commit with version `1.0.0`
- **THEN** `install` finds the release `1.0.0`

### Requirement: Version format
Every package version (manifest `version`, `forkOf.version`, `replaces.version`, `--version` arguments, catalog versions) MUST be `MAJOR.MINOR.PATCH` of decimal numbers without leading zeros; anything else, including pre-release and build suffixes, SHALL be refused with `invalid_version`. Versions SHALL be ordered numerically. Verified by `catalog::version_key_rejects_everything_else`.

#### Scenario: Numeric ordering
- **WHEN** releases `1.9.0` and `1.10.0` exist
- **THEN** `1.10.0` is the newer release

#### Scenario: Pre-release suffix is refused
- **WHEN** a manifest declares version `1.0.0-beta.1` or `01.0.0` or `1.0`
- **THEN** reading it fails with `invalid_version` (or `invalid_manifest` for a manifest file)

### Requirement: IDs are matched exactly
Operations on an installed package SHALL match the ID exactly as recorded in `state.json`; an ID that differs only in letter case MUST NOT resolve to the installed package, even on a case-insensitive file system. Verified by `uninstall::case_variant_id_never_touches_the_installed_plugin`.

#### Scenario: Case variant on APFS
- **WHEN** `vendor/module` is installed and the user runs `uninstall Vendor/Module`
- **THEN** the command fails with `repository_not_found` and `vendor/module` is untouched

### Requirement: Case-colliding IDs are refused
Installing, restoring or forking a package whose ID differs from an installed or retained package (or an existing workspace path) only in letter case SHALL be refused, independent of the file system's case sensitivity: install with `workspace` ("… differs only in letter case from installed …"), fork with `invalid_argument`. Verified by `workspace::ids_differing_only_in_case_are_refused`.

#### Scenario: Install after forget of a case variant
- **WHEN** `dan/p` is retained in the workspace and the user installs a repository whose manifest ID is `Dan/p`
- **THEN** the install fails with `workspace` and the workspace is unchanged
