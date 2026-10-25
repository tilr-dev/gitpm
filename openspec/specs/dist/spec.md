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
Managed Git deploys SHALL copy committed manifest-selected output (default `<path>/dist`; `"."`: `<path>`) from Git objects, never the working tree. Builds SHALL deploy cached verified output without Git/network. Both SHALL preserve nested folders and executable bits. Symlinks SHALL fail with `unsupported`; gitlinks SHALL become empty folders without fetching, with a path-naming warning to commit built files instead. Links follow "Linked deployments expose the author installation folder".

#### Scenario: Symlink inside dist
- **WHEN** a release's package dist contains a symlink
- **THEN** the deploy fails with `unsupported` ("symlinks are not supported: PATH")

#### Scenario: Uncommitted edits are not deployed
- **WHEN** a clone has uncommitted changes in `dist/` and the package is re-deployed by `enable`
- **THEN** `dist/<id>/` holds the committed files of the installed revision

#### Scenario: Submodule inside dist
- **WHEN** a release's `dist/vendor` is a submodule
- **THEN** install (and every other deploy) succeeds with an empty `dist/<id>/vendor/` folder and warns that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

#### Scenario: Executable file
- **WHEN** a release's `dist/bin/run` has the executable bit
- **THEN** the deployed `dist/<id>/bin/run` is executable

#### Scenario: export-ignore in the package repository
- **WHEN** the release commit's `.gitattributes` marks `dist/debug.js` `export-ignore`
- **THEN** `dist/<id>/debug.js` is deployed and `export-subst` placeholders stay as committed
- **AND** catalog creation, `install-catalog`, protected-package update, and `verify_catalog_release` hash or verify the staged `dist/<id>` export after `export_dist` and before committing the transaction, so each hash covers the deployed bytes

#### Scenario: Root or arbitrary output folder
- **WHEN** a release declares `dist: "."` or `dist: "output/vtf"`
- **THEN** install and subsequent deploys copy that committed folder, relative to the package root, including nested files and executable bits
- **AND** uncommitted edits and files outside the selected folder are not deployed

#### Scenario: Repair missing deployed build output
- **WHEN** an active build package has no deployed folder and sync-dist runs
- **THEN** its verified local output is deployed without Git or network access

### Requirement: A revision without dist is refused
A Git revision without its declared output folder (default dist/) SHALL fail with invalid_manifest, never deploying an empty folder. Messages SHALL name the missing folder and detection context below; substitute the declared path for dist in default messages. With buildRequired, errors SHALL explain that a committed build result is required and name buildCommand when supplied. Catalog builds SHALL use archive output without requiring that folder in Git. gitpm SHALL NOT run builds.

#### Scenario: Deploy from Git objects
- **WHEN** the commit being deployed has no declared output tree
- **THEN** the error is "release COMMIT has no [PATH/]dist/ folder"

#### Scenario: Checked-out root package
- **WHEN** a root package's checked-out folder has no declared output folder (`dist/` by default)
- **THEN** the error is "package must contain a ready dist/ directory"

#### Scenario: Package folder of a multi-package repository
- **WHEN** a folder listed by `gitpm.json` has no declared output folder (`dist/` by default)
- **THEN** that package is defective with "package FOLDER must contain a ready dist/ directory" (siblings are handled as in multi-package-repos)

#### Scenario: Release without dist
- **WHEN** the latest tag points at a commit without `dist/`
- **THEN** install fails with `invalid_manifest` and the workspace is unchanged

#### Scenario: Missing required build output
- **WHEN** a package declares `dist: "output"`, `buildRequired: true` and `buildCommand: "npm run build"` but output is absent
- **THEN** install or fork fails with invalid_manifest naming output and npm run build, without running the command or deploying an empty package

#### Scenario: Prebuilt release needs no committed output
- **WHEN** a build release manifest selects an output folder absent from its Git commit
- **THEN** installation deploys the verified build without accessing that commit

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
`disable ID` SHALL remove `dist/ID/` (for an active replacing package also drop the binding and deploy ORIGINAL when it is active then) and `enable ID` SHALL deploy it again from the clone. Their guards are specified in dependencies ("Dependents guard removal", "enable needs active dependencies"); this requirement covers only `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable acme/greeter` and then `enable acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `me/greeter` replaces `acme/greeter`, which is not installed, `app` depends on `acme/greeter`, and the user runs `disable me/greeter --force`
- **THEN** `dist/me/greeter/` is removed, the binding is dropped and nothing is deployed for `acme/greeter`; `app` is reported broken

#### Scenario: Refused enable
- **WHEN** `enable app` is refused because `lib` is disabled
- **THEN** `dist/app/` is not created

### Requirement: Inactive packages change state and clone only
`download`, and `install`, `update` and `rollback` of a replaced or disabled package, SHALL change state and clone only; its deployed folder SHALL appear only on `replace --back` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back`, `installing_a_package_that_is_replaced_deploys_nothing` and `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `acme/greeter` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/acme/greeter/` still does not exist, and a later `replace acme/greeter --back` deploys 0.2.0

#### Scenario: Download deploys nothing
- **WHEN** the user runs `download ./greeter`
- **THEN** `acme/greeter` is recorded disabled, its clone exists and `dist/acme/greeter/` does not

### Requirement: Forget, restore and uninstall settle dist
`forget ID` and `uninstall ID` SHALL remove `dist/ID/` (forget keeps the clone) and `restore ID` SHALL deploy it. When disabling, uninstalling or forgetting drops a replacement binding, the original is deployed again in the same transaction (rule in fork-replacement, "Active fork rules"; guard in dependencies, "Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Forget removes and restore deploys
- **WHEN** the user runs `forget acme/greeter` and then `restore acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again by the second

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, `download`, `download-catalog`, update, `update all`, rollback, replace, `replace --back`, enable, disable, forget, restore, uninstall, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed and enabled from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`; over a disabled record from the same source it enables the package and reconciles `dist/` (install-uninstall, "Install is idempotent for the same source")

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it; an active package below such a path SHALL be listed in `skipped` with the reason. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/acme` is a file and `acme/greeter` is active
- **THEN** a command on an unrelated package succeeds and leaves `acme/greeter` undeployed, while an operation that deploys or removes `acme/greeter` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: sync-dist with a blocked namespace
- **WHEN** `dist/acme` is a file, `acme/greeter` is active and the user runs `sync-dist`
- **THEN** `acme/greeter` is in `skipped` ("dist/acme is not a folder") and `sync-dist` fails with `not_deployed` instead of printing `dist/ already holds exactly the active packages.`
- **AND** the CLI hint tells the user to remove or rename `dist/acme` and run `gitpm sync-dist` again; advice to uninstall or forget a package whose clone is gone is given only for `clone_missing`

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

### Requirement: Undeployable packages do not block others
An active package whose clone or installed commit is missing SHALL be skipped by reconciliation so it does not block an unrelated command; the CLI SHALL warn `Warning: ID: dist not deployed (REASON)`. Any other deploy error SHALL fail the command. Verified by `a_missing_clone_is_reported_and_blocks_only_its_own_dist` and `skipped_dists_say_structurally_whether_the_clone_is_missing`.

#### Scenario: Clone deleted by hand
- **WHEN** the clone of an active package was deleted and the user enables another package
- **THEN** the enable succeeds and a warning names the package whose dist was not deployed and the way out (install-uninstall, "Removing a package whose clone is gone")

#### Scenario: Advice only for a missing clone
- **WHEN** a skipped item has `clone_missing` true (REASON `clone missing`), or its installed commit is missing instead (`clone_missing` false)
- **THEN** the first warning ends `; uninstall or forget ID to drop its record`, the second has no advice

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT reconcile or deploy anything in `dist/`, and neither opens the workspace (no lock, no crash recovery; transactions-locking, "Workspace lock"), so an interrupted commit is completed only by the next command that opens it.

#### Scenario: list on a 0.4 workspace
- **WHEN** a workspace written by gitpm 0.4 or 0.5 (state format 2) is listed
- **THEN** `list` fails with `unsupported` (workspace-layout, "State format versions") and `dist/` is unchanged

#### Scenario: list of a state edited by hand
- **WHEN** a package record in `state.json` was set to `"enabled": false` by hand while its `dist/<id>` still exists, and the user runs `gitpm list`
- **THEN** `list` reports the package with `active` false and `dist/` is unchanged; the next mutating command or `sync-dist` removes the folder

#### Scenario: list after an interrupted commit
- **WHEN** `.pm-txn/` holds a committed journal and the user runs `gitpm list`
- **THEN** `list` reports `state.json` as it is and leaves `.pm-txn/` and `dist/` alone

### Requirement: sync-dist repairs dist explicitly
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, `sync_dist_plan`, FFI `syncDist`/`syncDistPlan`) SHALL perform only the reconciliation, report each package deployed (`ID deployed to dist/ID`) or removed (`ID removed from dist/ID`; `would be …` with `--dry-run`) and sync every package it can. When an active package stays undeployed, `sync_dist` SHALL then fail with `not_deployed`, carrying the report; what it synced stays. Verified by `sync_dist_repairs_a_tree_written_by_an_older_gitpm`.

#### Scenario: Package that cannot be deployed
- **WHEN** the clone of an active package is missing and the user runs `sync-dist`
- **THEN** it syncs the others and fails with `not_deployed` ("N package(s) not deployed: IDS"); the CLI prints `Error kind: not_deployed` and a `Hint:` naming `gitpm uninstall ID` or `gitpm forget ID` (both drop a recorded package whose clone is gone), and exits with status 1

#### Scenario: Nothing to do
- **WHEN** `dist/` already matches the active packages (unmanaged paths may exist; they are only warned about)
- **THEN** `sync-dist` prints `dist/ already holds exactly the active packages.` and exits 0

#### Scenario: Dry run
- **WHEN** the user runs `sync-dist --dry-run` on a drifted workspace
- **THEN** it prints the `would be deployed to` / `would be removed from` lines and `Nothing was changed (--dry-run).` and changes nothing; `sync_dist_plan` lists undeployable packages in `skipped` and does not fail

### Requirement: Empty parent folders are removed
Removing a package SHALL also remove the folders it leaves empty (`dist/<namespace>`, `worktrees/<namespace>`, `worktrees`, `repos`). Verified by `removal_leaves_no_empty_namespace_or_clone_folders`.

#### Scenario: Last package of a namespace
- **WHEN** the only package of namespace `acme` is uninstalled
- **THEN** `dist/acme/`, `worktrees/acme/` and the unused `worktrees/` and `repos/` no longer exist

### Requirement: Mutating results carry the dist report
Every mutating library operation SHALL return, inside its result, the reconciliation report of its transaction: `deployed`, `removed`, `unmanaged` (paths under `dist/` left alone) and `skipped` (active packages not deployed, each with its reason). The library SHALL NOT print it; the CLI turns `unmanaged` and `skipped` into warnings (cli, "Warnings go to stderr") and an FFI host reads the same fields. Verified by `workspace::mutating_results_carry_the_dist_report`.

#### Scenario: Unknown folder in an install result
- **WHEN** `dist/custom/` belongs to no package and the user installs `acme/greeter`
- **THEN** the `InstallResult` lists `dist/custom` in its dist report's `unmanaged`, and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

#### Scenario: Skipped package in an enable result
- **WHEN** the clone of an active package is missing and the user enables another package
- **THEN** the enable result's dist report lists the package in `skipped` with the reason `clone missing`

### Requirement: Repeating enable and disable is idempotent
When dependency and replacement guards permit it, repeating enable or disable SHALL succeed with CLI exit 0, preserving workspace state and clone bytes. Already reconciled deployed files SHALL remain byte-identical; otherwise the operation SHALL reconcile dist as required by "Every mutating command reconciles all of dist".

#### Scenario: Repeat enable
- **WHEN** an enabled package is enabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical

#### Scenario: Repeat disable
- **WHEN** a disabled package is disabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical

#### Scenario: Repeat enable repairs missing dist
- **WHEN** an enabled package is enabled again while another active package has a missing deployed folder
- **THEN** the operation reconciles dist and restores that folder, preserving workspace state and clone bytes

### Requirement: Linked deployments expose the author installation folder
For link records, gitpm SHALL deploy `dist/<id>` as a symlink to the author-owned installation folder. It SHALL use the same `plugin.json.dist` folder selection as managed install: `"dist"` by default, or the configured package-relative folder (`"."` selects the package root). gitpm MUST NOT build, watch, copy or change its contents.

#### Scenario: Uncommitted author edits
- **WHEN** an active linked package's author edits a file in its installation folder
- **THEN** reading that file through `dist/<id>` returns the edited bytes without a commit or another gitpm operation

#### Scenario: Author-maintained build output
- **WHEN** the package declares an installation folder and the author writes new output there
- **THEN** `dist/<id>` resolves to that folder and exposes the new output without gitpm executing a build or watcher

### Requirement: Linked deployments follow activity without owning their target
Enable, disable, replacement, forget, restore, uninstall and reconciliation SHALL create or remove the linked package's deployed entry according to the existing active-package rules. They MUST NOT require a managed clone for a link record or delete or modify its external target. Missing linked folders SHALL be reported structurally as not deployed, with the link mode retained, and unrelated packages SHALL remain operable under the existing reconciliation rules.

#### Scenario: Disable and enable a link
- **WHEN** an enabled linked package is disabled and then enabled
- **THEN** its symlink is removed and recreated, and its external folder keeps its contents

#### Scenario: Author moves the target away
- **WHEN** a recorded linked folder no longer exists and reconciliation runs for another package
- **THEN** the report identifies the unavailable linked package and its link mode, and no clone or build output is created for it

#### Scenario: Existing link namespace becomes blocked
- **WHEN** the namespace directory of an existing link becomes a file or symlink and an independent package is changed
- **THEN** reconciliation reports the blocked link as not deployed with its descriptor, without traversing the blocked namespace or preventing the independent change
