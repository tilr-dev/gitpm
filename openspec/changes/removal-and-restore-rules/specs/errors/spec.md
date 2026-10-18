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
- **WHEN** `uninstall`, `uninstall_plan` or `restore` names an ID that has neither a record nor a clone in the workspace, or `restore` one whose clone is gone
- **THEN** the error kind is `repository_not_found` (they look for the clone, not the state entry); `uninstall` of a recorded package whose clone is gone succeeds instead (install-uninstall, "Removing a package whose clone is gone")
- **AND** this is covered by `workspace::every_operation_on_a_missing_id_fails_without_changes`

### Requirement: Display is the complete message
`Display` of an error SHALL be the complete human message in English. Library messages SHALL be neutral (no CLI command, flag or hosting service; the CLI adds hints, cli). Structured variants SHALL carry their data: `LocalChanges.items`, `HasDependents.dependents` and `.lost_work` (the lost work of the uninstall plan; empty for disable and forget), `IncompatibleReplacement.consumers`, `NotDeployed.sync`. Verified by `error::display_is_the_complete_message` and `error::messages_name_no_cli_command`.

#### Scenario: Has dependents
- **WHEN** disabling `lib` is refused because `app` depends on it
- **THEN** the error is `has_dependents`, its message names `app` and `dependents` is `["app"]`

#### Scenario: Neutral advice
- **WHEN** `init` is refused because the repository root is a package
- **THEN** the library message states the problem only (for example "the repository root is a package; a multi-package repository needs gitpm.json") and the CLI prints the command as a `Hint:` line
