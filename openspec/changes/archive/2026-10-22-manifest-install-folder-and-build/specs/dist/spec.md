## MODIFIED Requirements

### Requirement: Deploys copy the committed tree
Every deploy SHALL copy the resolved commit’s package folder selected by its committed manifest's `dist` (default `<path>/dist`; `"."` selects `<path>` itself) from Git objects, never the working tree, preserving nested folders and executable bits. Symlinks SHALL fail with `unsupported`. Gitlinks SHALL become empty directories without fetching submodules, with an actionable warning naming the path and advising committing built files instead.

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
A revision without its declared output folder (default dist/) SHALL fail with invalid_manifest, never deploying an empty folder. Messages SHALL name the missing folder and detection context as below; substitute the declared path for dist in default messages. With buildRequired, errors SHALL explain that a committed build result is required and name buildCommand when supplied. gitpm SHALL NOT run builds.

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
