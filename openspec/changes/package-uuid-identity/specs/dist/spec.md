## MODIFIED Requirements

### Requirement: dist holds exactly the active packages
The deployed folder `dist/<uuid>/` SHALL exist exactly when the package is active: installed, enabled and not replaced by another package. A host SHALL be able to load every package folder under `dist/` without consulting gitpm state while no transaction is committing (see the scenario). Verified by `dist_active.rs` (CLI) tests.

#### Scenario: Host reads during a commit
- **WHEN** a host reads `dist/` while `.pm-txn/` exists in the workspace root
- **THEN** it may miss a package, because a commit moves deployed folders one by one; hosts read `dist/` after the gitpm call returned

#### Scenario: Installed and enabled
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` is installed, enabled and not replaced
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` holds the files of its release's package dist

#### Scenario: Enabled but replaced
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` is replaced by `8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` does not exist, `dist/8f0b16d1-526c-56fd-bdae-b6b991bbcb76/` does, and `d135a7cf-1c63-549a-93cb-8f89656b2ced` stays `enabled: true` with `active: false` in `list`

### Requirement: Empty parent folders are removed
Removing a package SHALL remove its UUID deployment entry and remove `repos` when no clone remains. `dist` SHALL remain the workspace deployment root; there SHALL be no namespace directories to clean up.

#### Scenario: Last package removed
- **WHEN** the last package is uninstalled
- **THEN** its `dist/<uuid>/` entry and unused `repos/` are absent and `dist/` remains

#### Scenario: Last package of a namespace
- **WHEN** the last installed package with a particular alias is uninstalled
- **THEN** only its UUID deployment entry and unused clone storage are removed; no namespace folder exists and `dist/` remains

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A foreign symlink or file at `dist/<uuid>` SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written through it; an active package at such a path SHALL be listed in `skipped` with the reason. A recorded linked deployment's managed leaf symlink follows the linked deployment rules. Verified by `a_uuid_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced` is a file and `d135a7cf-1c63-549a-93cb-8f89656b2ced` is active
- **THEN** a command on an unrelated package succeeds and leaves `d135a7cf-1c63-549a-93cb-8f89656b2ced` undeployed, while an operation that deploys or removes `d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: sync-dist with a blocked namespace
- **WHEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced` is a file, `d135a7cf-1c63-549a-93cb-8f89656b2ced` is active and the user runs `sync-dist`
- **THEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` is in `skipped` ("dist/d135a7cf-1c63-549a-93cb-8f89656b2ced is not a folder") and `sync-dist` fails with `not_deployed` instead of printing `dist/ already holds exactly the active packages.`
- **AND** the CLI hint tells the user to remove or rename `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced` and run `gitpm sync-dist` again; advice to uninstall or forget a package whose clone is gone is given only for `clone_missing`

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

### Requirement: Replace removes the original's dist
`replace ORIGINAL --with FORK` SHALL remove `dist/ORIGINAL/` and SHALL NOT copy anything of FORK into it; `dist/FORK/` stays. Its preconditions (FORK installed, enabled and declaring `replaces`) are specified in fork-replacement ("Replacement is explicit and global"). Verified by `replace_removes_the_original_dist_and_back_restores_it_from_the_clone`.

#### Scenario: Replace
- **WHEN** the user runs `replace d135a7cf-1c63-549a-93cb-8f89656b2ced --with 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is removed and `dist/8f0b16d1-526c-56fd-bdae-b6b991bbcb76/` still holds the fork's files

### Requirement: Switching back redeploys the original
`replace ORIGINAL --with ORIGINAL` SHALL deploy `dist/ORIGINAL/` again from ORIGINAL's clone at its installed commit (and package folder), without a checkout of a shared clone; the former replacing package stays deployed while it is installed and enabled.

#### Scenario: Back to the original
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` was replaced by `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` and the user runs `replace d135a7cf-1c63-549a-93cb-8f89656b2ced --with d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is deployed again and `dist/8f0b16d1-526c-56fd-bdae-b6b991bbcb76/` remains

### Requirement: Enable and disable deploy and remove
`disable ID` SHALL remove `dist/ID/` (for an active replacing package also drop the binding and deploy ORIGINAL when it is active then) and `enable ID` SHALL deploy it again from the clone. Their guards are specified in dependencies ("Dependents guard removal", "enable needs active dependencies"); this requirement covers only `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable d135a7cf-1c63-549a-93cb-8f89656b2ced` and then `enable d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces `d135a7cf-1c63-549a-93cb-8f89656b2ced`, which is not installed, `app` depends on `d135a7cf-1c63-549a-93cb-8f89656b2ced`, and the user runs `disable 8f0b16d1-526c-56fd-bdae-b6b991bbcb76 --force`
- **THEN** `dist/8f0b16d1-526c-56fd-bdae-b6b991bbcb76/` is removed, the binding is dropped and nothing is deployed for `d135a7cf-1c63-549a-93cb-8f89656b2ced`; `app` is reported broken

#### Scenario: Refused enable
- **WHEN** `enable app` is refused because `lib` is disabled
- **THEN** `dist/app/` is not created

### Requirement: Inactive packages change state and clone only
`download`, and `install`, `update` and `rollback` of a replaced or disabled package, SHALL change state and clone only; its deployed folder SHALL appear only on `replace ORIGINAL --with ORIGINAL` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back`, `installing_a_package_that_is_replaced_deploys_nothing` and `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `d135a7cf-1c63-549a-93cb-8f89656b2ced` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` still does not exist, and a later `replace d135a7cf-1c63-549a-93cb-8f89656b2ced --with d135a7cf-1c63-549a-93cb-8f89656b2ced` deploys 0.2.0

#### Scenario: Download deploys nothing
- **WHEN** the user runs `download ./greeter`
- **THEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` is recorded disabled, its clone exists and `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` does not

### Requirement: Forget, restore and uninstall settle dist
`forget ID` and `uninstall ID` SHALL remove `dist/ID/` (forget keeps the clone) and `restore ID` SHALL deploy it. When disabling, uninstalling or forgetting drops a replacement binding, the original is deployed again in the same transaction (rule in fork-replacement, "Active fork rules"; guard in dependencies, "Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Forget removes and restore deploys
- **WHEN** the user runs `forget d135a7cf-1c63-549a-93cb-8f89656b2ced` and then `restore d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is removed by the first and deployed again by the second

### Requirement: Mutating results carry the dist report
Every mutating library operation SHALL return, inside its result, the reconciliation report of its transaction: `deployed`, `removed`, `unmanaged` (paths under `dist/` left alone) and `skipped` (active packages not deployed, each with its reason). The library SHALL NOT print it; the CLI turns `unmanaged` and `skipped` into warnings (cli, "Warnings go to stderr") and an FFI host reads the same fields. Verified by `workspace::mutating_results_carry_the_dist_report`.

#### Scenario: Unknown folder in an install result
- **WHEN** `dist/custom/` belongs to no package and the user installs `d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** the `InstallResult` lists `dist/custom` in its dist report's `unmanaged`, and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

#### Scenario: Skipped package in an enable result
- **WHEN** the clone of an active package is missing and the user enables another package
- **THEN** the enable result's dist report lists the package in `skipped` with the reason `clone missing`

### Requirement: Linked deployments follow activity without owning their target
Enable, disable, replacement, forget, restore, uninstall and reconciliation SHALL create or remove the linked package's deployed entry according to the existing active-package rules. They MUST NOT require a managed clone for a link record or delete or modify its external target. Missing linked folders SHALL be reported structurally as not deployed, with the link mode retained, and unrelated packages SHALL remain operable under the existing reconciliation rules.

#### Scenario: Disable and enable a link
- **WHEN** an enabled linked package is disabled and then enabled
- **THEN** its symlink is removed and recreated, and its external folder keeps its contents

#### Scenario: Author moves the target away
- **WHEN** a recorded linked folder no longer exists and reconciliation runs for another package
- **THEN** the report identifies the unavailable linked package and its link mode, and no clone or build output is created for it

#### Scenario: Existing link namespace becomes blocked
- **WHEN** the UUID deployment entry of an existing link is replaced by a file or a foreign symlink and an independent package is changed
- **THEN** reconciliation reports the blocked link as not deployed with its descriptor, without traversing the blocked entry or preventing the independent change
