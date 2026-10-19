## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy the package dist `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree, unchanged by Git attributes. Nested folders and the executable bit SHALL be kept; symlinks SHALL be refused with `unsupported`, while a submodule comes out as an empty folder. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits`.

#### Scenario: Symlink inside dist
- **WHEN** a release's package dist contains a symlink
- **THEN** the deploy fails with `unsupported` ("symlinks and submodules are not supported: PATH"; the message also names submodules, which are not refused)

#### Scenario: Uncommitted edits are not deployed
- **WHEN** a clone has uncommitted changes in `dist/` and the package is re-deployed by `enable`
- **THEN** `dist/<id>/` holds the committed files of the installed revision

#### Scenario: Submodule inside dist
- **WHEN** a release's `dist/vendor` is a submodule
- **THEN** the deploy succeeds and `dist/<id>/vendor/` is an empty folder

#### Scenario: Executable file
- **WHEN** a release's `dist/bin/run` has the executable bit
- **THEN** the deployed `dist/<id>/bin/run` is executable

#### Scenario: export-ignore in the package repository
- **WHEN** the release commit's `.gitattributes` marks `dist/debug.js` `export-ignore`
- **THEN** `dist/<id>/debug.js` is deployed and `export-subst` placeholders stay as committed
- **AND** catalog creation, `install-catalog`, protected-package update, and `verify_catalog_release` hash or verify the staged `dist/<id>` export after `export_dist` and before committing the transaction, so each hash covers the deployed bytes
