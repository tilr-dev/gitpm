## MODIFIED Requirements

### Requirement: Errors name the package
Errors about a package SHALL name its ID (and version where relevant) in the library message, including repeated forget/uninstall, unknown IDs, catalog conflicts, update and rollback refusals and the per-package refusals of `update all`; the CLI SHALL add nothing to the message, and the FFI `PmError` SHALL carry the ID in `id`. Verified by `workspace::every_operation_on_a_missing_id_fails_without_changes` and `error::package_errors_name_the_package`.

#### Scenario: Repeated forget
- **WHEN** `forget acme/greeter` is run twice
- **THEN** the second error message contains `acme/greeter`

#### Scenario: Update and rollback refusals
- **WHEN** a branch-mode update finds diverged history, a rollback finds a manifest that does not match the recorded revision, or the origin's manifest declares another ID
- **THEN** each message names `acme/greeter` (for example "acme/greeter: history of main diverged; automatic update refused", "acme/greeter: rollback manifest mismatch", "acme/greeter: package id changed upstream to acme/other"), and the FFI error carries `id` `acme/greeter`

#### Scenario: Local work in update all
- **WHEN** `update all` is refused because the clone of `acme/greeter` has unpublished local work
- **THEN** the `local_changes` message names `acme/greeter` and its items, and the CLI prints it unchanged

#### Scenario: Remaining release and branch refusals
- **WHEN** an update of `acme/greeter` is refused because its newest release tag is broken or its recorded origin branch no longer exists
- **THEN** the `version_conflict` message names `acme/greeter` and the FFI error carries its `id`
