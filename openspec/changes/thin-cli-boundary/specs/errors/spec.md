## MODIFIED Requirements

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
- **WHEN** `uninstall`, `uninstall_plan` or `restore` names an ID that has no clone or record in the workspace
- **THEN** the error kind is `repository_not_found` (they look for the clone, not the state entry)
- **AND** this is covered by `workspace::every_operation_on_a_missing_id_fails_without_changes`

### Requirement: Display is the complete message
`Display` of an error SHALL be the complete human message in English. Library messages SHALL be neutral: they SHALL NOT name a CLI command, flag or hosting service; the CLI adds command hints per error kind (cli, "Errors and exit codes"). Structured variants SHALL carry their data: `LocalChanges.items`, `HasDependents.dependents`, `IncompatibleReplacement.consumers`, `NotDeployed.skipped`. Verified by `error::display_is_the_complete_message` and `error::messages_name_no_cli_command`.

#### Scenario: Has dependents
- **WHEN** disabling `lib` is refused because `app` depends on it
- **THEN** the error is `has_dependents`, its message names `app` and `dependents` is `["app"]`

#### Scenario: Neutral advice
- **WHEN** a scaffold refusal or a replacement refusal used to advise `init --multi` or `replace ORIGINAL --back`
- **THEN** the library message states the problem only (for example "the repository root is a package; a multi-package repository needs gitpm.json") and the CLI prints the command as a `Hint:` line

### Requirement: Failing operations leave the workspace unchanged
Every failing `Manager` operation, update and rollback included, SHALL leave the workspace byte-identical (tree hash: files and state) with no transaction directory left behind. This requirement is the single source for that guarantee; the staging and journal mechanism is in transactions-locking. Verified by `workspace::every_operation_rejects_invalid_ids_without_changes` and `workspace::every_operation_on_a_missing_id_fails_without_changes`.

#### Scenario: Fresh workspace root
- **WHEN** an operation fails on a workspace root that did not exist
- **THEN** a refusal of the Options creates nothing (workspace-layout, "Options are validated before anything is created"), while a failure after opening leaves the empty workspace that opening created

#### Scenario: Partial fork --replace
- **WHEN** a step of `fork --replace` fails
- **THEN** the steps that succeeded stay (fork-replacement, "Fork folder is never removed after a partial failure"); likewise `sync-dist` keeps what it synced when it fails with `not_deployed` (dist)

#### Scenario: Invalid ID everywhere
- **WHEN** each operation is called with `../x`
- **THEN** each fails with `invalid_id` and the workspace tree hash is unchanged

#### Scenario: Network failure mid-update
- **WHEN** the fetch of an update fails
- **THEN** the command fails with `git` and the workspace tree hash is unchanged
- **AND** this is covered by `failed_update_keeps_files_and_state` and `workspace::update_refusals_leave_the_workspace_unchanged`
