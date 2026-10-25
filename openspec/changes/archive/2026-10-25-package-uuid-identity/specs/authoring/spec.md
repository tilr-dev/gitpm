## MODIFIED Requirements

### Requirement: init creates a package with its first release
`init NAME --into DIR` SHALL scaffold a package with a manifest (a newly generated UUID `id`, `"name": "NAME"`, `version` 0.1.0, `"title": "NAME"`), a demo `dist/index.js`, one commit and the tag `v0.1.0`. A `DIR` that does not exist SHALL become a single-package repository. `--into` is required and nothing SHALL ever be overwritten. Verified by `scaffold::init_into_a_new_folder_makes_a_tagged_single_package_repository` and `scaffold::init_never_overwrites_and_validates_ids`.

#### Scenario: Scaffold files and commit
- **WHEN** `init hello --into hello` creates a new single-package repository
- **THEN** it also writes `.gitignore` and `README.md`, and the one commit is `Create hello 0.1.0` (a package added to a multi-package repository gets `Create UUID VERSION` using the current repository version)

#### Scenario: New package repository
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** `hello` is a Git repository at tag `v0.1.0` and the output contains `Created:`, `ID: UUID` matching the manifest and `Version 0.1.0: Git commit and tag v0.1.0.`

#### Scenario: init without --into
- **WHEN** the user runs `gitpm init hello`
- **THEN** it fails with `init needs --into DIR: …` and creates nothing

#### Scenario: Former alias
- **WHEN** the user runs `gitpm create hello --into hello`
- **THEN** it fails with `unknown command: create; use --help` and creates nothing

#### Scenario: Generated identity survives release
- **WHEN** `init hello --into hello` creates a package and `commit` releases it again
- **THEN** its manifest has a generated UUID and name `hello`, and the UUID is unchanged by the release

### Requirement: init options
`init --multi` SHALL create a multi-package repository in a new or empty folder. `--no-release` SHALL omit the initial tag and print a later release command using 0.1.0. Adding to an existing repository SHALL omit the tag regardless of this option and print a command requiring a new version. Failure SHALL remove a repository gitpm created, leaving an adopted empty folder empty.

#### Scenario: Failure in a repository gitpm created
- **WHEN** the tag fails during a single-package `init` or `init --multi` into a new or empty folder
- **THEN** the new repository is removed (an adopted empty folder is left empty) and the error is returned

#### Scenario: Start a multi-package repository from nothing
- **WHEN** the user runs `gitpm init runtime --into repos/p --multi --no-release`
- **THEN** `repos/p` has `gitpm.json` and `packages/<uuid>`, one commit and no tag, and the suggested release command uses 0.1.0

#### Scenario: Tag fails when adding to an existing repository
- **WHEN** a package is added to an existing multi-package repository at version 1.2.0, with or without `--no-release`, and a tag command would fail
- **THEN** only the package addition is committed; no tag command runs or fails, and the suggested release command requires a new version

### Requirement: init into a multi-package repository
`init` into an existing repository SHALL add a package at its shared version (0.1.0 if empty), commit only the new folder and config, and create no tag. It SHALL use the first `PREFIX/*` pattern with the generated UUID as the folder name, else add `packages/<uuid>` to the config. A repository without config or root package SHALL get `packages/*`. Dirty config, invalid roots and duplicate folders/UUIDs SHALL be refused with `invalid_argument`, changing nothing.

#### Scenario: Root is a package
- **WHEN** the repository root has a manifest and no `gitpm.json`
- **THEN** init is refused

#### Scenario: Existing folder that is not a repository
- **WHEN** `DIR` exists and is not a Git repository (an empty folder without `--multi`)
- **THEN** init is refused with `… exists but is not a Git repository; pass a new folder or a repository`

#### Scenario: Not the repository root
- **WHEN** `DIR` is a subfolder of a repository
- **THEN** init is refused with `pass the repository root`

#### Scenario: Second package with the same name
- **WHEN** the repository already has a package named `ui` and the user runs `init ui --into REPO`
- **THEN** it adds another package with name `ui`, a fresh UUID and a distinct UUID folder, preserving the first package

#### Scenario: Add a package after a release
- **WHEN** an existing repository has packages at 1.2.0 and the tag `v1.2.0`, and the author adds another package with `init`
- **THEN** the new package has version 1.2.0, no tag is created or moved, and it is absent from the existing release
- **AND** `gitpm commit REPO 1.3.0` sets every package to 1.3.0 and creates `v1.3.0`, releasing the new package too

### Requirement: commit releases a version
`commit REPO VERSION [--package ID]` SHALL set every manifest version, stage all changes, commit `Release VERSION` and create one tag using `git tag -m MESSAGE`, honoring `tag.gpgSign` without an editor. It SHALL never push. REPO must be the root; a selector must match a package (`invalid_argument`). The selector only chooses the returned/printed ID; without it all packages are still released. Output SHALL be `ID committed VERSION (local commit and tag TAG)`.

#### Scenario: Invalid version
- **WHEN** the author runs `gitpm commit REPO 1.0` or `gitpm commit REPO 1.0.0+build`
- **THEN** it fails with `invalid_version` (format in package-ids) and nothing changes

#### Scenario: Wrong-typed metadata
- **WHEN** the manifest has `"tags": "ui"`
- **THEN** it fails with `invalid_manifest` (rule in plugin-metadata, "Lenient reading, strict authoring")

#### Scenario: Release one package of a multi-package repository
- **WHEN** the author runs `gitpm commit ./tilr-packages 0.2.0 --package 76a9b591-60f3-5c7f-9e1c-31648fc73bfd`
- **THEN** every package is set to 0.2.0, one commit and one tag `v0.2.0` are created, and it prints `76a9b591-60f3-5c7f-9e1c-31648fc73bfd committed 0.2.0 (local commit and tag v0.2.0)`

#### Scenario: Release a pre-release
- **WHEN** the author runs `gitpm commit ./hello 1.3.0-beta.1`
- **THEN** it creates the tag `v1.3.0-beta.1`; consumers get it only when they ask for it (updates, "Pre-releases are opt-in")

#### Scenario: Signing configured without an editor
- **WHEN** `tag.gpgSign=true` and a usable signing key and Git identity are configured, and `commit` releases a repository with multiple packages
- **THEN** the single `vVERSION` tag is signed, its message is supplied with `-m`, and no editor is needed

#### Scenario: Release a multi-package repository without a selector
- **WHEN** the author runs `gitpm commit REPO 0.3.0` in a repository with several packages
- **THEN** all packages become 0.3.0 and share one release commit and one tag `v0.3.0`

### Requirement: commit returns what it released
`commit_package` SHALL return `{package, version, tag}` of the repository release it made; `package` is the selected package ID and does not restrict which packages are released (the tag as in package-ids, "Release tag names"), and the CLI SHALL print its line from that result without computing the tag itself. Verified by `scaffold::commit_package_returns_package_version_and_tag`.

#### Scenario: Result of a release
- **WHEN** the author runs `gitpm commit ./tilr-plugins 0.2.0 --package 76a9b591-60f3-5c7f-9e1c-31648fc73bfd`
- **THEN** the library returns package `76a9b591-60f3-5c7f-9e1c-31648fc73bfd`, version `0.2.0` and tag `v0.2.0`, and the CLI prints `76a9b591-60f3-5c7f-9e1c-31648fc73bfd committed 0.2.0 (local commit and tag v0.2.0)`
