## MODIFIED Requirements

### Requirement: Forgotten packages of shared clones
Forgetting a package of a shared clone SHALL keep a `forgotten` record (format in workspace-layout) that still holds its bare repository and Git worktree; `restore` SHALL record it again under the rules of install-uninstall ("Restore preconditions") and deploy it when active; `uninstall` of the forgotten package SHALL remove the record, its worktree and, if last, the bare repository. Verified by `a_forgotten_shared_package_keeps_its_clone_until_uninstalled`.

#### Scenario: Shared clone gone
- **WHEN** the shared clone of a forgotten package was deleted
- **THEN** `uninstall` drops the record, and `restore` fails with `repository_not_found` (the clone is what restore needs, as for any restore without a clone); the CLI adds a `Hint:` line naming `gitpm uninstall ID` and a new install
- **AND** this is covered by `a_forgotten_record_whose_clone_is_gone_can_be_uninstalled`

#### Scenario: Restore from shared clone
- **WHEN** `tilr/store` was forgotten and the user runs `restore tilr/store`
- **THEN** it is recorded in state at its recorded revision with its recorded `enabled` flag, and `dist/tilr/store/` is deployed when it is enabled
