# dist Specification

## Purpose

Defines the workspace folder `dist/`, the deployed output a host loads: which packages have a deployed folder `dist/<id>` (exactly the active ones), how deployed folders are copied from the package dist in Git objects, how every mutating command and `sync-dist` reconcile the tree, and what gitpm leaves alone.

## Requirements

### Requirement: dist holds exactly the active packages
The deployed folder `dist/<namespace>/<name>/` SHALL exist exactly when the package is active: installed, enabled and not replaced by another package. A host SHALL be able to load every package folder under `dist/` without consulting gitpm state while no transaction is committing (see the scenario). Verified by `dist_active.rs` (CLI) tests.

#### Scenario: Host reads during a commit
- **WHEN** a host reads `dist/` while `.pm-txn/` exists in the workspace root
- **THEN** it may miss a package, because a commit moves deployed folders one by one; hosts read `dist/` after the gitpm call returned

#### Scenario: Installed and enabled
- **WHEN** `acme/greeter` is installed, enabled and not replaced
- **THEN** `dist/acme/greeter/` holds the files of its release's package dist

#### Scenario: Enabled but replaced
- **WHEN** `acme/greeter` is replaced by `me/greeter`
- **THEN** `dist/acme/greeter/` does not exist, `dist/me/greeter/` does, and `acme/greeter` stays `enabled: true` with `active: false` in `list`

### Requirement: Deploys copy the committed tree
Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy the package dist `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree. Nested folders and the executable bit SHALL be kept; symlinks SHALL be refused with `unsupported`, while a submodule comes out as an empty folder. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits`.

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

### Requirement: A revision without dist is refused
Single source for the missing package dist refusal: a revision whose package folder has no `dist/` SHALL be refused with `invalid_manifest` and SHALL NOT be deployed as an empty folder. The message names where it was detected (see the scenarios). Verified by `workspace::a_revision_without_dist_is_never_deployed_empty` and `packages::invalid_package_sets_are_refused`.

#### Scenario: Deploy from Git objects
- **WHEN** the commit being deployed has no `[PATH/]dist` tree
- **THEN** the error is "release COMMIT has no [PATH/]dist/ folder"

#### Scenario: Checked-out root package
- **WHEN** a root package's checked-out folder has no `dist/`
- **THEN** the error is "package must contain a ready dist/ directory"

#### Scenario: Package folder of a multi-package repository
- **WHEN** a folder listed by `gitpm.json` has no `dist/`
- **THEN** that package is defective with "package FOLDER must contain a ready dist/ directory" (siblings are handled as in multi-package-repos)

#### Scenario: Release without dist
- **WHEN** the latest tag points at a commit without `dist/`
- **THEN** install fails with `invalid_manifest` and the workspace is unchanged

### Requirement: Replace removes the original's dist
`replace ORIGINAL --with FORK` SHALL remove `dist/ORIGINAL/` and SHALL NOT copy anything of FORK into it; `dist/FORK/` stays. Its preconditions (FORK installed, enabled and declaring `replaces`) are specified in fork-replacement ("Replacement is explicit and global"). Verified by `replace_removes_the_original_dist_and_back_restores_it_from_the_clone`.

#### Scenario: Replace
- **WHEN** the user runs `replace acme/greeter --with me/greeter`
- **THEN** `dist/acme/greeter/` is removed and `dist/me/greeter/` still holds the fork's files

### Requirement: Switching back redeploys the original
`replace ORIGINAL --back` SHALL deploy `dist/ORIGINAL/` again from ORIGINAL's clone at its installed commit (and package folder), without a checkout of a shared clone; the former replacing package stays deployed while it is installed and enabled.

#### Scenario: Back to the original
- **WHEN** `acme/greeter` was replaced by `me/greeter` and the user runs `replace acme/greeter --back`
- **THEN** `dist/acme/greeter/` is deployed again and `dist/me/greeter/` remains

### Requirement: Enable and disable deploy and remove
`disable ID` SHALL remove `dist/ID/` and `enable ID` SHALL deploy it again from the clone. The `has_dependents` guard on disabling a replacing package or a dependency is specified in dependencies ("Dependents guard removal"); this requirement covers only its effect on `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable acme/greeter` and then `enable acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `me/greeter` replaces `acme/greeter` and the user runs `disable me/greeter --force`
- **THEN** `dist/me/greeter/` is removed and, because `acme/greeter` stays replaced, `dist/acme/greeter/` is not deployed

### Requirement: Inactive packages change state and clone only
`install`, `update` and `rollback` of a replaced or disabled package SHALL change state and clone only; its deployed folder SHALL appear only on `replace --back` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back` and `installing_a_package_that_is_replaced_deploys_nothing`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `acme/greeter` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/acme/greeter/` still does not exist, and a later `replace acme/greeter --back` deploys 0.2.0

### Requirement: Forget, restore and uninstall settle dist
`forget ID` and `uninstall ID` SHALL remove `dist/ID/` (forget keeps the clone) and `restore ID` SHALL deploy it. When uninstalling or forgetting drops a replacement binding, the original is deployed again in the same transaction (rule in fork-replacement, "Active fork rules"). Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Forget removes and restore deploys
- **WHEN** the user runs `forget acme/greeter` and then `restore acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again by the second

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, update, `update all`, rollback, replace, `replace --back`, enable, disable, forget, restore, uninstall, `fork --replace`, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/acme` is a file and `acme/greeter` is active
- **THEN** a command on an unrelated package succeeds and leaves `acme/greeter` undeployed, while an operation that deploys or removes `acme/greeter` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

### Requirement: Undeployable packages do not block others
An active package whose clone or installed commit is missing SHALL be skipped by reconciliation so it does not block an unrelated command; the CLI SHALL warn `Warning: ID: dist not deployed (clone missing); run \`gitpm sync-dist\``. Any other deploy error SHALL fail the command. Verified by `a_missing_clone_is_reported_and_blocks_only_its_own_dist`.

#### Scenario: Clone deleted by hand
- **WHEN** the clone of an active package was deleted and the user enables another package
- **THEN** the enable succeeds and a warning names the package whose dist was not deployed

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT reconcile or deploy anything in `dist/`. `list` opens the workspace (transactions-locking, "Workspace lock"), so it completes an interrupted commit found there ("Crash recovery"); `check` opens nothing.

#### Scenario: list on a 0.4 workspace
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user runs `list`
- **THEN** `dist/` is unchanged

### Requirement: sync-dist repairs dist explicitly
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, `sync_dist_plan`, FFI `syncDist`/`syncDistPlan`) SHALL perform only the reconciliation, report each package deployed (`ID deployed to dist/ID`) or removed (`ID removed from dist/ID`; `would be …` with `--dry-run`), sync every package it can, then fail with exit status 1 naming those it could not deploy. Verified by `sync_dist_repairs_a_tree_written_by_an_older_gitpm`.

#### Scenario: Package that cannot be deployed
- **WHEN** the clone of an active package is missing and the user runs `sync-dist`
- **THEN** it syncs the others and fails with `sync-dist: N package(s) not deployed: IDS`, a CLI error without `Error kind`

#### Scenario: Nothing to do
- **WHEN** `dist/` already matches the active packages (unmanaged paths may exist; they are only warned about)
- **THEN** `sync-dist` prints `dist/ already holds exactly the active packages.` and exits 0

#### Scenario: Dry run
- **WHEN** the user runs `sync-dist --dry-run` on a drifted workspace
- **THEN** it prints the `would be deployed to` / `would be removed from` lines and `Nothing was changed (--dry-run).` and changes nothing

### Requirement: Empty parent folders are removed
Removing a package SHALL also remove the folders it leaves empty (`dist/<namespace>`, `packages/<namespace>`, `repos`). Verified by `removal_leaves_no_empty_namespace_or_clone_folders`.

#### Scenario: Last package of a namespace
- **WHEN** the only package of namespace `acme` is uninstalled
- **THEN** `dist/acme/` and `packages/acme/` no longer exist
