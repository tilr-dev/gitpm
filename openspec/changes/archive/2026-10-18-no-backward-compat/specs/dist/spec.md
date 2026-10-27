## MODIFIED Requirements

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

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/acme` is a file and `acme/greeter` is active
- **THEN** a command on an unrelated package succeeds and leaves `acme/greeter` undeployed, while an operation that deploys or removes `acme/greeter` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

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

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, update, `update all`, rollback, replace, `replace --back`, enable, disable, forget, restore, uninstall, `fork --replace`, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT reconcile or deploy anything in `dist/`. `list` opens the workspace (transactions-locking, "Workspace lock"), so it completes an interrupted commit found there ("Crash recovery"); `check` opens nothing.

#### Scenario: list on a 0.4 workspace
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user runs `list`
- **THEN** `dist/` is unchanged
