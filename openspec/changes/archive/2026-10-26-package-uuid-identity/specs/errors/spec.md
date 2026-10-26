## MODIFIED Requirements

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
