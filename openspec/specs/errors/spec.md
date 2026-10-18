# errors Specification

## Purpose

Defines the error contract shared by the library, CLI and FFI: one typed error with a stable machine-readable kind and a complete human message, and the rule that messages name the package concerned.

## Requirements

### Requirement: Stable error kinds
Every failure SHALL be a `gitpm::Error` whose `kind()` is one of: `not_installed`, `already_installed`, `local_changes`, `locked`, `invalid_id`, `invalid_manifest`, `invalid_version`, `invalid_argument`, `version_conflict`, `no_previous_version`, `no_releases`, `repository_not_found`, `workspace`, `catalog`, `git`, `http`, `auth`, `unsupported`, `dependency_conflict`, `has_dependents`, `incompatible_replacement`, `invalid_data`, `io`. A kind MUST never change for an existing variant.

#### Scenario: Where the less obvious kinds arise
- **WHEN** a caller sees `http`, `auth`, `catalog` or `unsupported`
- **THEN** `http` is a failed catalog request (catalog, "Catalog locations and transport"), `auth` a failing `auth check` (git-safety), `catalog` invalid catalog data including an unknown catalog schema, and `unsupported` for example a newer `state.json` format (workspace-layout) or a symlink in a package dist (dist)

#### Scenario: Failures without a kind
- **WHEN** the CLI itself refuses a usage error, a missing catalog, a declined prompt or `sync-dist` leaving packages undeployed
- **THEN** these are not library errors and print no `Error kind` (cli, "Errors and exit codes")

#### Scenario: Future variants
- **WHEN** a caller matches on `gitpm::Error`
- **THEN** the enum is `#[non_exhaustive]`, so the match needs a wildcard arm for variants added later

#### Scenario: Kind of a missing package
- **WHEN** `update`, `rollback`, `enable`, `disable`, `forget` or `fork` names an ID that is not installed
- **THEN** the error kind is `not_installed` ("plugin ID is not installed")
- **AND** this is covered by `error::every_variant_has_its_stable_kind` and `workspace::every_operation_on_a_missing_id_fails_without_changes`

#### Scenario: Removal of an unknown package
- **WHEN** `uninstall`, `uninstall_plan` or `restore` names an ID that has no clone or record in the workspace
- **THEN** the error kind is `repository_not_found` (they look for the clone, not the state entry)
- **AND** this is covered by `workspace::every_operation_on_a_missing_id_fails_without_changes`

### Requirement: README lists exactly the kinds
README "Error kinds" SHALL list exactly the kinds the library defines. Verified by `error::readme_lists_exactly_the_error_kinds`.

#### Scenario: New variant without docs
- **WHEN** a variant is added without updating README
- **THEN** the unit test fails

### Requirement: Display is the complete message
`Display` of an error SHALL be the complete human message in English. Library messages MAY name a gitpm command as advice (for example `init --multi` in scaffold refusals, `replace ORIGINAL --back` when disabling an active replacing package). Structured variants SHALL carry their data: `LocalChanges.items`, `HasDependents.dependents`, `IncompatibleReplacement.consumers`. Verified by `error::display_is_the_complete_message`.

#### Scenario: Has dependents
- **WHEN** disabling `lib` is refused because `app` depends on it
- **THEN** the error is `has_dependents`, its message names `app` and `dependents` is `["app"]`

### Requirement: Conversions choose documented kinds
I/O errors (and `StripPrefixError`, `SystemTimeError`) SHALL map to `io`, JSON and UTF-8 errors to `invalid_data`, integer parse errors to `invalid_version`. Verified by `error::conversions_choose_the_documented_kind`.

#### Scenario: Unreadable file
- **WHEN** a workspace file cannot be read because of its permissions
- **THEN** the error kind is `io` (a `state.json` that cannot be parsed is `invalid_data`, see workspace-layout)

### Requirement: Errors name the package
Errors about a package SHALL name its ID (and version where relevant) in the message, including repeated forget/uninstall, unknown IDs and catalog conflicts. Verified by `workspace::every_operation_on_a_missing_id_fails_without_changes`.

#### Scenario: Repeated forget
- **WHEN** `forget acme/greeter` is run twice
- **THEN** the second error message contains `acme/greeter`

### Requirement: Failing operations leave the workspace unchanged
Every failing `Manager` operation, update and rollback included, SHALL leave the workspace byte-identical (tree hash: files and state) with no transaction directory left behind. This requirement is the single source for that guarantee; the staging and journal mechanism is in transactions-locking. Verified by `workspace::every_operation_rejects_invalid_ids_without_changes` and `workspace::every_operation_on_a_missing_id_fails_without_changes`.

#### Scenario: Fresh workspace root
- **WHEN** an operation fails on a workspace root that did not exist
- **THEN** the root that opening created (workspace-layout) stays

#### Scenario: Partial fork --replace
- **WHEN** a step of `fork --replace` fails
- **THEN** the steps that succeeded stay (fork-replacement, "Fork folder is never removed after a partial failure")

#### Scenario: Invalid ID everywhere
- **WHEN** each operation is called with `../x`
- **THEN** each fails with `invalid_id` and the workspace tree hash is unchanged

#### Scenario: Network failure mid-update
- **WHEN** the fetch of an update fails
- **THEN** the command fails with `git` and the workspace tree hash is unchanged
- **AND** this is covered by `failed_update_keeps_files_and_state` and `workspace::update_refusals_leave_the_workspace_unchanged`
