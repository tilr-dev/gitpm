## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Managed Git deploys SHALL copy committed manifest-selected output (default `<path>/dist`; `"."`: `<path>`) from Git objects, never the working tree. Builds SHALL deploy cached verified output without Git/network. Both SHALL preserve nested folders and executable bits. Links follow "Linked deployments expose the author installation folder".

Verified by `every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits`.

#### Scenario: Uncommitted edits are not deployed
- **WHEN** a clone has uncommitted changes in `dist/` and the package is re-deployed by `enable`
- **THEN** `dist/<id>/` holds the committed files of the installed revision

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
A Git revision without its declared output folder (default dist/) SHALL fail with invalid_manifest, never deploying an empty folder. Catalog builds SHALL use archive output without requiring that folder in Git. gitpm SHALL NOT run builds.

Verified by `workspace::a_revision_without_dist_is_never_deployed_empty` and `packages::invalid_package_sets_are_refused`.

#### Scenario: Release without dist
- **WHEN** the latest tag points at a commit without `dist/`
- **THEN** install fails with `invalid_manifest` and the workspace is unchanged

#### Scenario: Prebuilt release needs no committed output
- **WHEN** a build release manifest selects an output folder absent from its Git commit
- **THEN** installation deploys the verified build without accessing that commit

## ADDED Requirements

### Requirement: Symlinks and submodules in dist
Symlinks SHALL fail with `unsupported`; gitlinks SHALL become empty folders without fetching, with a path-naming warning to commit built files instead.

Verified by `a_submodule_in_dist_deploys_an_empty_folder`.

#### Scenario: Symlink inside dist
- **WHEN** a release's package dist contains a symlink
- **THEN** the deploy fails with `unsupported` ("symlinks are not supported: PATH")

#### Scenario: Submodule inside dist
- **WHEN** a release's `dist/vendor` is a submodule
- **THEN** install (and every other deploy) succeeds with an empty `dist/<id>/vendor/` folder and warns that `dist/vendor` is a Git submodule whose files will not be installed, probably a mistake; commit built files to dist instead of a submodule

### Requirement: Missing output diagnostics
Messages SHALL name the missing folder and detection context below; substitute the declared path for dist in default messages. With buildRequired, errors SHALL explain that a committed build result is required and name buildCommand when supplied.

Verified by `declared_output_missing_required_build_is_actionable_and_atomic`.

#### Scenario: Deploy from Git objects
- **WHEN** the commit being deployed has no declared output tree
- **THEN** the error is "release COMMIT has no [PATH/]dist/ folder"

#### Scenario: Checked-out root package
- **WHEN** a root package's checked-out folder has no declared output folder (`dist/` by default)
- **THEN** the error is "package must contain a ready dist/ directory"

#### Scenario: Package folder of a multi-package repository
- **WHEN** a folder listed by `gitpm.json` has no declared output folder (`dist/` by default)
- **THEN** that package is defective with "package FOLDER must contain a ready dist/ directory" (siblings are handled as in multi-package-repos)

#### Scenario: Missing required build output
- **WHEN** a package declares `dist: "output"`, `buildRequired: true` and `buildCommand: "npm run build"` but output is absent
- **THEN** install or fork fails with invalid_manifest naming output and npm run build, without running the command or deploying an empty package
