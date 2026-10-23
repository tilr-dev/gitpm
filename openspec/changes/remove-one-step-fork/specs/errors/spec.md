## MODIFIED Requirements

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
