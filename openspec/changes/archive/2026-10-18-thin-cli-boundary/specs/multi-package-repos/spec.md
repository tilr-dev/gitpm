## MODIFIED Requirements

### Requirement: Forgotten packages of shared clones
Forgetting a package of a shared clone SHALL keep a `forgotten` record (format in workspace-layout) that still holds the clone; `restore` SHALL record it in state again at its recorded revision (checking the clone out there; `local_changes` when the clone has lost work) and deploy it; `uninstall` of the forgotten package SHALL remove the record and, if last, the clone. Verified by `a_forgotten_shared_package_keeps_its_clone_until_uninstalled`.

#### Scenario: Shared clone gone
- **WHEN** the shared clone of a forgotten package was deleted
- **THEN** `uninstall` drops the record, and `restore` fails with `repository_not_found` (the clone is what restore needs, as for any restore without a clone); the CLI adds a `Hint:` line naming `gitpm uninstall ID` and a new install
- **AND** this is covered by `a_forgotten_record_whose_clone_is_gone_can_be_uninstalled`

#### Scenario: Restore from shared clone
- **WHEN** `tilr/store` was forgotten and the user runs `restore tilr/store`
- **THEN** it is recorded in state at its recorded revision and `dist/tilr/store/` is deployed
