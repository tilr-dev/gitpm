## MODIFIED Requirements

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it; an active package below such a path SHALL be listed in `skipped` with the reason. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/acme` is a file and `acme/greeter` is active
- **THEN** a command on an unrelated package succeeds and leaves `acme/greeter` undeployed, while an operation that deploys or removes `acme/greeter` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: sync-dist with a blocked namespace
- **WHEN** `dist/acme` is a file, `acme/greeter` is active and the user runs `sync-dist`
- **THEN** `acme/greeter` is in `skipped` ("dist/acme is not a folder") and `sync-dist` fails with `not_deployed` instead of printing `dist/ already holds exactly the active packages.`

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

### Requirement: Deploys copy the committed tree
Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy the package dist `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree, unchanged by Git attributes. Nested folders and the executable bit SHALL be kept; symlinks and submodules (gitlinks) SHALL be refused with `unsupported` before anything is deployed. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits` and `a_submodule_in_dist_is_refused`.

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

#### Scenario: export-ignore in the package repository
- **WHEN** the release commit's `.gitattributes` marks `dist/debug.js` `export-ignore`
- **THEN** `dist/<id>/debug.js` is deployed (likewise `export-subst` placeholders stay as committed), and the catalog's file hashes (catalog, "Release record") cover it, so install verification and deploy see the same files
