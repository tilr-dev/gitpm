# errors Specification

## Purpose

Defines the error contract shared by the library, CLI and FFI: one typed error with a stable machine-readable kind and a complete human message, and the rule that messages name the package concerned.

## Requirements

### Requirement: Stable error kinds
Every failure SHALL be a `gitpm::Error` whose `kind()` is one of: not_installed, already_installed, local_changes, locked, invalid_id, invalid_manifest, invalid_version, invalid_argument, version_conflict, no_previous_version, no_releases, repository_not_found, workspace, catalog, git, http, auth, unsupported, dependency_conflict, has_dependents, incompatible_replacement, not_deployed, invalid_data, io. A kind MUST never change for an existing variant.

#### Scenario: Where the less obvious kinds arise
- **WHEN** a caller sees `http`, `auth`, `catalog`, `unsupported` or `not_deployed`
- **THEN** `http` is a failed catalog request (catalog, "Catalog locations and transport"), `auth` a failing `auth check` (git-safety), `catalog` invalid catalog data including an unknown catalog schema or a v1 catalog, `unsupported` for example a `state.json` of another format (workspace-layout) or a symlink in a package dist (dist), and `not_deployed` a `sync-dist` that left active packages undeployed (dist)

#### Scenario: Failures without a kind
- **WHEN** the CLI itself refuses a usage error, a missing catalog or a declined prompt
- **THEN** these are not library errors and print no `Error kind` (cli, "Errors and exit codes")

#### Scenario: Future variants
- **WHEN** a caller matches on `gitpm::Error`
- **THEN** the enum is `#[non_exhaustive]`, so the match needs a wildcard arm for variants added later

#### Scenario: Kind of a missing package
- **WHEN** `update`, `rollback`, `enable`, `disable`, `forget` or `fork` names an ID that is not installed
- **THEN** the error kind is `not_installed` ("package ID is not installed")
- **AND** this is covered by `error::every_variant_has_its_stable_kind` and `workspace::every_operation_on_a_missing_id_fails_without_changes`

#### Scenario: Removal of an unknown package
- **WHEN** `uninstall`, `uninstall_plan` or `restore` names an ID that has neither a record nor a clone in the workspace, or `restore` one whose clone is gone
- **THEN** the error kind is `repository_not_found` (they look for the clone, not the state entry); `uninstall` of a recorded package whose clone is gone succeeds instead (install-uninstall, "Removing a package whose clone is gone")
- **AND** this is covered by `workspace::every_operation_on_a_missing_id_fails_without_changes`

### Requirement: README lists exactly the kinds
README "Error kinds" SHALL list exactly the kinds the library defines. Verified by `error::readme_lists_exactly_the_error_kinds`.

#### Scenario: New variant without docs
- **WHEN** a variant is added without updating README
- **THEN** the unit test fails

### Requirement: Display is the complete message
`Display` of an error SHALL be the complete human message in English. Library messages SHALL be neutral (no CLI command, flag or hosting service; the CLI adds hints, cli). Structured variants SHALL carry their data: `LocalChanges.items`, `HasDependents.dependents` and `.lost_work` (the uninstall plan's; empty otherwise), `IncompatibleReplacement.consumers`, `NotDeployed.sync`. Verified by `error::display_is_the_complete_message` and `error::messages_name_no_cli_command`.

#### Scenario: Has dependents
- **WHEN** disabling `lib` is refused because `app` depends on it
- **THEN** the error is `has_dependents`, its message names `app` and `dependents` is `["app"]`

#### Scenario: Neutral advice
- **WHEN** `init` is refused because the repository root is a package
- **THEN** the library message states the problem only (for example "the repository root is a package; a multi-package repository needs gitpm.json") and the CLI prints the command as a `Hint:` line

### Requirement: Conversions choose documented kinds
I/O errors (and `StripPrefixError`, `SystemTimeError`) SHALL map to `io`, JSON and UTF-8 errors to `invalid_data`, integer parse errors to `invalid_version`. Verified by `error::conversions_choose_the_documented_kind`.

#### Scenario: Unreadable file
- **WHEN** a workspace file cannot be read because of its permissions
- **THEN** the error kind is `io` (a `state.json` that cannot be parsed is `invalid_data`, see workspace-layout)

### Requirement: Errors name the package
Errors about a package SHALL name its ID (and version where relevant) in the library message, including repeated forget/uninstall, unknown IDs, catalog conflicts, update and rollback refusals and the per-package refusals of `update all`; the CLI SHALL add nothing to the message, and the FFI `PmError` SHALL carry the ID in `id`. Verified by `workspace::every_operation_on_a_missing_id_fails_without_changes` and `error::package_errors_name_the_package`.

#### Scenario: Repeated forget
- **WHEN** `forget d135a7cf-1c63-549a-93cb-8f89656b2ced` is run twice
- **THEN** the second error message contains `d135a7cf-1c63-549a-93cb-8f89656b2ced`

#### Scenario: Update and rollback refusals
- **WHEN** a branch-mode update finds diverged history, a rollback finds a manifest that does not match the recorded revision, or the origin's manifest declares another ID
- **THEN** each message names `d135a7cf-1c63-549a-93cb-8f89656b2ced` (for example "d135a7cf-1c63-549a-93cb-8f89656b2ced: history of main diverged; automatic update refused", "d135a7cf-1c63-549a-93cb-8f89656b2ced: rollback manifest mismatch", "d135a7cf-1c63-549a-93cb-8f89656b2ced: package id changed upstream to cde71d60-4de0-5f8d-a3c9-26b40d4af02a"), and the FFI error carries `id` `d135a7cf-1c63-549a-93cb-8f89656b2ced`

#### Scenario: Local work in update all
- **WHEN** `update all` is refused because the clone of `d135a7cf-1c63-549a-93cb-8f89656b2ced` has unpublished local work
- **THEN** the `local_changes` message names `d135a7cf-1c63-549a-93cb-8f89656b2ced` and its items, and the CLI prints it unchanged

#### Scenario: Remaining release and branch refusals
- **WHEN** an update of `d135a7cf-1c63-549a-93cb-8f89656b2ced` is refused because its newest release tag is broken or its recorded origin branch no longer exists
- **THEN** the `version_conflict` message names `d135a7cf-1c63-549a-93cb-8f89656b2ced` and the FFI error carries its `id`

#### Scenario: Refusal from a pulled dependency
- **WHEN** `update d135a7cf-1c63-549a-93cb-8f89656b2ced` or `update all` pulls the new dependency `cfbc4d17-8707-5571-853d-c9136a7f9f21` and the catalog release of `cfbc4d17-8707-5571-853d-c9136a7f9f21` has a broken `plugin.json` or an invalid version requirement
- **THEN** the `invalid_manifest` message starts with `cfbc4d17-8707-5571-853d-c9136a7f9f21:` and the FFI error carries `id` `cfbc4d17-8707-5571-853d-c9136a7f9f21`, not `d135a7cf-1c63-549a-93cb-8f89656b2ced` (`update_refusal_from_a_pulled_dependency_names_the_dependency`, `update_all_refusal_from_a_pulled_dependency_names_the_dependency`, `update_refusal_from_a_pulled_dependency_requirement_names_the_dependency`)

#### Scenario: Invalid requirement in a newer revision
- **WHEN** `update all` finds a newer release tag, branch commit or catalog release of `d135a7cf-1c63-549a-93cb-8f89656b2ced` whose manifest has an invalid version requirement
- **THEN** the `invalid_manifest` message starts with `d135a7cf-1c63-549a-93cb-8f89656b2ced:` and the FFI error carries `id` `d135a7cf-1c63-549a-93cb-8f89656b2ced` (`update_all_invalid_requirement_names_the_package`, `update_all_invalid_requirement_in_the_catalog_names_the_package`, `workspace::update_all_invalid_requirement_on_the_branch_names_the_package`)

### Requirement: Failing operations leave the workspace unchanged
Every failing `Manager` operation, update and rollback included, SHALL leave the workspace byte-identical (tree hash: files and state) with no transaction directory left behind. This requirement is the single source for that guarantee; the staging and journal mechanism is in transactions-locking. Verified by `workspace::every_operation_rejects_invalid_ids_without_changes` and `workspace::every_operation_on_a_missing_id_fails_without_changes`.

#### Scenario: Fresh workspace root
- **WHEN** an operation fails on a workspace root that did not exist
- **THEN** a refusal of the Options creates nothing (workspace-layout, "Options are validated before anything is created"), while a failure after opening leaves the empty workspace that opening created

#### Scenario: Invalid ID everywhere
- **WHEN** each operation is called with `../x`
- **THEN** each fails with `invalid_id` and the workspace tree hash is unchanged

#### Scenario: Network failure mid-update
- **WHEN** the fetch of an update fails
- **THEN** the command fails with `git` and the workspace tree hash is unchanged
- **AND** this is covered by `failed_update_keeps_files_and_state` and `workspace::update_refusals_leave_the_workspace_unchanged`

#### Scenario: Partial dist sync
- **WHEN** `sync-dist` fails with `not_deployed`
- **THEN** it keeps what it synced as specified in dist

#### Scenario: Partial fork --replace
- **WHEN** a caller attempts the removed composed fork operation through the CLI
- **THEN** the CLI rejects the removed option before creating the destination or changing the workspace; no partial release, installation or replacement occurs
