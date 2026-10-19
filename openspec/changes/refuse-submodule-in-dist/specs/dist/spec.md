## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy the package dist `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree. Nested folders and the executable bit SHALL be kept; symlinks and submodules (gitlinks) SHALL be refused with `unsupported` before anything is deployed. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits` and `a_submodule_in_dist_is_refused`.

#### Scenario: Symlink inside dist
- **WHEN** a release's package dist contains a symlink
- **THEN** the deploy fails with `unsupported` ("symlinks and submodules are not supported: PATH")

#### Scenario: Uncommitted edits are not deployed
- **WHEN** a clone has uncommitted changes in `dist/` and the package is re-deployed by `enable`
- **THEN** `dist/<id>/` holds the committed files of the installed revision

#### Scenario: Submodule inside dist
- **WHEN** a release's `dist/vendor` is a submodule
- **THEN** install (and every other deploy) fails with `unsupported` ("symlinks and submodules are not supported: dist/vendor") and the workspace is unchanged, instead of deploying an empty `vendor/` folder

#### Scenario: Executable file
- **WHEN** a release's `dist/bin/run` has the executable bit
- **THEN** the deployed `dist/<id>/bin/run` is executable
