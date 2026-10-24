## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Managed deploys SHALL copy committed manifest-selected output (default `<path>/dist`; `"."`: `<path>`) from Git objects, never the working tree. A verified build for that commit is a release cache, deployed without Git/network. It SHALL preserve nested folders and executable bits; symlinks fail with `unsupported`; gitlinks become empty folders with a path-naming actionable warning, without fetching. Links follow "Linked deployments expose the author installation folder".

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

## ADDED Requirements

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
