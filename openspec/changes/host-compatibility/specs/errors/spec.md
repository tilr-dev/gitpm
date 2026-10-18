## MODIFIED Requirements

### Requirement: Stable error kinds
Every failure SHALL be a `gitpm::Error` whose `kind()` is one of: not_installed, already_installed, local_changes, locked, invalid_id, invalid_manifest, invalid_version, invalid_argument, version_conflict, no_previous_version, no_releases, repository_not_found, workspace, catalog, git, http, auth, unsupported, dependency_conflict, has_dependents, incompatible_replacement, incompatible_host, not_deployed, invalid_data, io. A kind MUST never change for an existing variant.

#### Scenario: Where the less obvious kinds arise
- **WHEN** a caller sees `http`, `auth`, `catalog`, `unsupported`, `not_deployed` or `incompatible_host`
- **THEN** `http` is a failed catalog request (catalog, "Catalog locations and transport"), `auth` a failing `auth check` (git-safety), `catalog` invalid catalog data including an unknown catalog schema or a v1 catalog, `unsupported` for example a `state.json` of another format (workspace-layout) or a symlink in a package dist (dist), `not_deployed` a `sync-dist` that left active packages undeployed (dist), and `incompatible_host` a release that does not fit the caller's host (updates, "Releases are chosen for the host")

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

#### Scenario: Host does not fit
- **WHEN** no release of a package fits the caller's host
- **THEN** the error kind is `incompatible_host` and the message names the package, the host and the needed range (updates, "Releases are chosen for the host")
