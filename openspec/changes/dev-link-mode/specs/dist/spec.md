## ADDED Requirements

### Requirement: Linked deployments expose the author installation folder
For link records, gitpm SHALL deploy `dist/<id>` as a symlink to the author-owned installation folder. It SHALL use the manifest installation-folder contract: package root by default, or the configured folder. gitpm MUST NOT build, watch, copy or change its contents.

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

## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Every managed deploy (install, update, rollback, restore, `replace --back`, enable, reconcile) SHALL copy the package dist `<path>/dist` of the resolved commit from the clone's Git objects, never from the working tree; links deploy as "Linked deployments expose the author installation folder". Nested folders and the executable bit SHALL be kept; symlinks SHALL be refused with `unsupported`, while a submodule comes out as an empty folder. Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits`.

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
Single source for the missing package dist refusal: a managed revision whose package folder has no `dist/` SHALL be refused with `invalid_manifest` and SHALL NOT be deployed as an empty folder. The message names where it was detected (see the scenarios). Verified by `workspace::a_revision_without_dist_is_never_deployed_empty` and `packages::invalid_package_sets_are_refused`.

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
