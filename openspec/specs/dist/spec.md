# dist Specification

## Purpose

Defines `dist/`, the deployed output a host loads: which packages have a `dist/<id>` folder (exactly the active ones), how folders are deployed from Git objects, how every mutating command and `sync-dist` reconcile the tree, and what gitpm leaves alone.

## Requirements

### Requirement: dist holds exactly the active packages
`dist/<namespace>/<name>/` (`dist/<id>/` for a legacy unscoped ID) SHALL exist exactly when the package is installed, enabled and not replaced by another package (`State::is_active`). A host SHALL be able to load every package folder under `dist/` without consulting gitpm state. Verified by `dist_active.rs` (CLI) tests.

#### Scenario: Installed and enabled
- **WHEN** `acme/greeter` is installed, enabled and not replaced
- **THEN** `dist/acme/greeter/` holds the files of its release's `dist/`

#### Scenario: Enabled but replaced
- **WHEN** `acme/greeter` is replaced by `me/greeter`
- **THEN** `dist/acme/greeter/` does not exist, `dist/me/greeter/` does, and `acme/greeter` stays `enabled: true` with `active: false` in `list`

### Requirement: Deploys copy the committed tree
Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree. Nested folders and the executable bit SHALL be kept; symlinks SHALL be refused (`unsupported`, "symlinks and submodules are not supported: PATH"), while a submodule comes out as an empty folder. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits`.

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
Single source for the missing-`dist/` refusal: a revision whose package folder has no `dist/` SHALL be refused with `invalid_manifest` and SHALL NOT be deployed as an empty folder. The message names where it was detected (see the scenarios). Verified by `workspace::a_revision_without_dist_is_never_deployed_empty` and `packages::invalid_package_sets_are_refused`.

#### Scenario: Deploy from Git objects
- **WHEN** the commit being deployed has no `[PATH/]dist` tree
- **THEN** the error is "release COMMIT has no [PATH/]dist/ folder"

#### Scenario: Checked-out root package
- **WHEN** a root package's checked-out folder has no `dist/`
- **THEN** the error is "plugin must contain a ready dist/ directory"

#### Scenario: Package folder of a multi-package repository
- **WHEN** a folder listed by `gitpm.json` has no `dist/`
- **THEN** that package is defective with "package FOLDER must contain a ready dist/ directory" (siblings are handled as in multi-package-repos)

#### Scenario: Release without dist
- **WHEN** the latest tag points at a commit without `dist/`
- **THEN** install fails with `invalid_manifest` and the workspace is unchanged

### Requirement: Replace removes the original's dist
`replace X --with Y` SHALL remove `dist/X/` and SHALL NOT copy anything of `Y` into it; `dist/Y/` stays. `Y` MUST be enabled, otherwise the replace is refused with `invalid_argument` ("Y is disabled; enable it first"). Verified by `replace_removes_the_original_dist_and_back_restores_it_from_the_clone`.

#### Scenario: Replace with a disabled fork
- **WHEN** `me/greeter` is disabled and the user runs `replace acme/greeter --with me/greeter`
- **THEN** the command fails with `invalid_argument` and `dist/acme/greeter/` stays

### Requirement: Switching back redeploys the original
`replace X --back` SHALL deploy `dist/X/` again from `X`'s clone at its installed commit (and package folder), without a checkout of a shared clone; the former replacement stays deployed while it is installed and enabled.

#### Scenario: Back to the original
- **WHEN** `acme/greeter` was replaced by `me/greeter` and the user runs `replace acme/greeter --back`
- **THEN** `dist/acme/greeter/` is deployed again and `dist/me/greeter/` remains

### Requirement: Enable and disable deploy and remove
`disable X` SHALL remove `dist/X/` and `enable X` SHALL deploy it again from the clone. The `has_dependents` guard on disabling a replacer or a dependency is specified in dependencies ("Dependents guard removal"); this requirement covers only its effect on `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable acme/greeter` and then `enable acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `me/greeter` replaces `acme/greeter` and the user runs `disable me/greeter --force`
- **THEN** `dist/me/greeter/` is removed and, because `acme/greeter` stays replaced, `dist/acme/greeter/` is not deployed

### Requirement: Inactive packages change state and clone only
`install`, `update` and `rollback` of a replaced or disabled package SHALL change state and clone only; its `dist/` SHALL appear only on `replace --back` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back` and `installing_a_package_that_is_replaced_deploys_nothing`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `acme/greeter` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/acme/greeter/` still does not exist, and a later `replace acme/greeter --back` deploys 0.2.0

### Requirement: Forget, restore and uninstall settle dist
`forget X` SHALL remove `dist/X/` (the clone stays) and `restore X` SHALL deploy it. When uninstalling or forgetting drops a replacement binding (rule in fork-replacement, "Active fork rules"), each original it replaced SHALL be deployed again in the same transaction. Verified by `uninstalling_or_forgetting_the_replacement_deploys_the_original_again`.

#### Scenario: Uninstall the fork
- **WHEN** `me/greeter` replaces `acme/greeter` and the user uninstalls `me/greeter`
- **THEN** the replacement is dropped and `dist/acme/greeter/` is deployed again

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, update, rollback, replace, enable, disable, forget, restore, uninstall, `fork --replace`) SHALL, inside its transaction, deploy missing folders of active packages and remove folders of inactive known packages. An existing folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** a workspace written by gitpm 0.4 holds `dist/` of a disabled package and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it; the active packages of that namespace are then not deployed, without failing the command. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known plugin or is not a folder; left as it is`

### Requirement: Undeployable packages do not block others
An active package whose clone or installed commit is missing SHALL be skipped by reconciliation so it does not block an unrelated command; the CLI SHALL warn `Warning: ID: dist not deployed (clone missing); run \`gitpm sync-dist\``. Any other deploy error SHALL fail the command. Verified by `a_missing_clone_is_reported_and_blocks_only_its_own_dist`.

#### Scenario: Clone deleted by hand
- **WHEN** the clone of an active package was deleted and the user enables another package
- **THEN** the enable succeeds and a warning names the package whose dist was not deployed

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT change `dist/`.

#### Scenario: list on a 0.4 workspace
- **WHEN** a workspace written by gitpm 0.4 is listed by gitpm 0.5
- **THEN** `dist/` is unchanged

### Requirement: sync-dist repairs dist explicitly
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, `sync_dist_plan`, FFI `syncDist`/`syncDistPlan`) SHALL perform only the reconciliation, report each package deployed (`ID deployed to dist/ID`) or removed (`ID removed from dist/ID`; `would be …` with `--dry-run`), sync every package it can, then exit non-zero naming the packages it could not deploy (`sync-dist: N package(s) not deployed: IDS`, a CLI error without `Error kind`). Verified by `sync_dist_repairs_a_tree_written_by_an_older_gitpm`.

#### Scenario: Nothing to do
- **WHEN** `dist/` already matches the active packages (unmanaged paths may exist; they are only warned about)
- **THEN** `sync-dist` prints `dist/ already holds exactly the active plugins.` and exits 0

#### Scenario: Dry run
- **WHEN** the user runs `sync-dist --dry-run` on a drifted workspace
- **THEN** it prints the `would be deployed to` / `would be removed from` lines and `Nothing was changed (--dry-run).` and changes nothing

### Requirement: Empty parent folders are removed
Removing a package SHALL also remove the folders it leaves empty (`dist/<namespace>`, `plugins/<namespace>`, `repos`). Verified by `removal_leaves_no_empty_namespace_or_clone_folders`.

#### Scenario: Last package of a namespace
- **WHEN** the only package of namespace `acme` is uninstalled
- **THEN** `dist/acme/` and `plugins/acme/` no longer exist
