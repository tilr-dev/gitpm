## MODIFIED Requirements

### Requirement: Same ID from another source is refused
Installing an ID that is installed from a different source (including a `file://` URL against a plain path, or a URL with and without `.git`) SHALL fail with `already_installed`. Installing an ID that is forgotten but retained SHALL be refused with `already_installed` until it is restored or uninstalled. Verified by `forgotten_repository_blocks_reinstall_until_restored`.

#### Scenario: Other source
- **WHEN** `acme/greeter` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

#### Scenario: Forgotten but retained
- **WHEN** `acme/greeter` was forgotten and its clone `repos/<key>` is retained, and the user installs it again
- **THEN** the command fails with `already_installed` ("acme/greeter was forgotten and its clone is kept") and the workspace is unchanged
- **AND** the message names no command; the CLI adds a `Hint:` line naming `gitpm restore ID` and `gitpm uninstall ID` (the same holds for a forgotten package of a shared clone)

### Requirement: Restore preconditions
`restore ID` SHALL record the package again from its `forgotten` record (Git or build) as it was; branch mode takes HEAD when it moved on along the origin's branch (dependencies checked). It SHALL refuse with `local_changes` on lost work and with `already_installed` when the ID is recorded, and print `ID restored V`. Verified by `workspace::restore_keeps_the_forgotten_record`, `restore_in_a_shared_clone_ignores_another_branch_s_revision` and `branch_mode_restore_checks_dependencies`.

#### Scenario: Restored record
- **WHEN** a clean retained clone of `acme/greeter`, forgotten while disabled and bound to a catalog, is restored
- **THEN** state records the forgotten revisions, `catalog`, `enabled: false` and mode unchanged, and nothing is deployed because it is disabled

#### Scenario: Dirty retained clone
- **WHEN** `acme/greeter` was forgotten, a file in `repos/<key>` was edited and the user runs `restore acme/greeter`
- **THEN** the command fails with `local_changes` and the package stays out of state

#### Scenario: Unpublished commit in a retained clone
- **WHEN** a commit was made in the retained clone that no remote branch contains, and the user restores it
- **THEN** restore fails with `local_changes` listing `UnpublishedCommit`, also in branch mode

#### Scenario: Branch mode restore
- **WHEN** a package in branch mode `main` was forgotten and its clone's HEAD is a published commit newer than the recorded one
- **THEN** `restore` records that HEAD as `current` and keeps branch mode `main`

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a recorded Git package whose clone is gone SHALL succeed without confirmation, drop the record and deployed folder, keep no forgotten record, and retain dependent guards (dependencies, "Dependents guard removal"). Builds intentionally have no clone and SHALL follow the cache lifecycle below. Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `repos/<key>` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git

#### Scenario: Managed build output is not lost Git work
- **WHEN** a build is forgotten or uninstalled
- **THEN** managed cached output needs no lost-work confirmation; forget retains its record and cached output for restore, while uninstall removes the cache, subject to dependent guards
