## ADDED Requirements

### Requirement: Linked deployments expose the author installation folder
For link records, gitpm SHALL deploy `dist/<id>` as a symlink to the author-owned installation folder. It SHALL use the same `plugin.json.dist` folder selection as managed install: package root by default, or the configured package-relative folder. gitpm MUST NOT build, watch, copy or change its contents.

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
Managed deploys SHALL copy the committed manifest's `dist` folder (default package root) from Git objects, preserving nested folders and executable bits. Links follow "Linked deployments expose the author installation folder". Managed-output symlinks SHALL fail with `unsupported`; Gitlinks SHALL become empty directories without fetching submodules, with a warning naming the path and advising committed built files.

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

### Requirement: A revision without dist is refused
A revision without its declared output folder (default package root) SHALL fail with invalid_manifest, never deploying an empty folder. Messages SHALL name the missing folder and detection context as below; substitute the declared path for dist in default messages. With buildRequired, errors SHALL explain that a committed build result is required and name buildCommand when supplied. gitpm SHALL NOT run builds.

#### Scenario: Deploy from Git objects
- **WHEN** the commit being deployed has no configured `dist` output tree (for example `dist: "dist"`)
- **THEN** the error is "release COMMIT has no [PATH/]dist/ folder"

#### Scenario: Checked-out root package
- **WHEN** a root package's checked-out folder lacks its configured `dist: "dist"` output
- **THEN** the error is "package must contain a ready dist/ directory"

#### Scenario: Package folder of a multi-package repository
- **WHEN** a folder listed by `gitpm.json` lacks its configured `dist: "dist"` output
- **THEN** that package is defective with "package FOLDER must contain a ready dist/ directory" (siblings are handled as in multi-package-repos)

#### Scenario: Release without dist
- **WHEN** the latest tag points at a commit without its declared output folder
- **THEN** install fails with `invalid_manifest` and the workspace is unchanged

#### Scenario: Missing required build output
- **WHEN** a package declares `dist: "output"`, `buildRequired: true` and `buildCommand: "npm run build"` but output is absent
- **THEN** install or fork fails with invalid_manifest naming output and npm run build, without running the command or deploying an empty package

