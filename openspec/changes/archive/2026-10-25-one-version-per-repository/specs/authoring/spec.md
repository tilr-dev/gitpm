## MODIFIED Requirements

### Requirement: init creates a package with its first release
When creating a repository, `init NAMESPACE/NAME --into DIR` SHALL scaffold a package with a manifest (`id`, `version` 0.1.0, `"title": "NAME"`), a demo `dist/index.js`, one commit and the tag `v0.1.0`. A `DIR` that does not exist SHALL become a single-package repository. `--into` is required and nothing SHALL ever be overwritten. Verified by `scaffold::init_into_a_new_folder_makes_a_tagged_single_package_repository` and `scaffold::init_never_overwrites_and_validates_ids`.

#### Scenario: Scaffold files and commit
- **WHEN** `init acme/hello --into hello` creates a new single-package repository
- **THEN** it also writes `.gitignore` and `README.md`, and the one commit is `Create hello 0.1.0` (a package added to a multi-package repository gets `Create acme/hello VERSION` using the current repository version)

#### Scenario: New package repository
- **WHEN** the user runs `gitpm init acme/hello --into hello`
- **THEN** `hello` is a Git repository at tag `v0.1.0` and the output contains `Created:`, `ID: acme/hello` and `Version 0.1.0: Git commit and tag v0.1.0.`

#### Scenario: init without --into
- **WHEN** the user runs `gitpm init acme/hello`
- **THEN** it fails with `init needs --into DIR: …` and creates nothing

#### Scenario: Former alias
- **WHEN** the user runs `gitpm create acme/hello --into hello`
- **THEN** it fails with `unknown command: create; use --help` and creates nothing

### Requirement: init into a multi-package repository
`init` into an existing repository SHALL add a package at its shared version (0.1.0 if empty), commit only the new folder and config, and create no tag. It SHALL use the first `PREFIX/*` pattern, else add `packages/NAME` to the config. A repository without config or root package SHALL get `packages/*`. Dirty config, invalid roots and duplicate folders/IDs SHALL be refused with `invalid_argument`, changing nothing.

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
- **WHEN** `a/ui` exists in the repository and the user runs `init b/ui --into REPO`
- **THEN** it is refused with `invalid_argument` naming the package that has the folder
- **AND** this is covered by `scaffold::packages_are_added_to_a_multi_package_repository_and_released_by_id` and `scaffold::new_packages_follow_the_first_folder_pattern`
- **AND** an ID differing only in letter case from an existing package is refused as in package-ids ("Case-colliding IDs are refused")

#### Scenario: Add a package after a release
- **WHEN** an existing repository has packages at 1.2.0 and the tag `v1.2.0`, and the author adds another package with `init`
- **THEN** the new package has version 1.2.0, no tag is created or moved, and it is absent from the existing release
- **AND** `gitpm commit REPO 1.3.0` sets every package to 1.3.0 and creates `v1.3.0`, releasing the new package too

### Requirement: init options
`init --multi` SHALL create a multi-package repository in a new or empty folder. `--no-release` SHALL omit the initial tag and print a later release command using 0.1.0. Adding to an existing repository SHALL omit the tag regardless of this option and print a command requiring a new version. Failure SHALL remove a repository gitpm created, leaving an adopted empty folder empty.

#### Scenario: Failure in a repository gitpm created
- **WHEN** the tag fails during a single-package `init` or `init --multi` into a new or empty folder
- **THEN** the new repository is removed (an adopted empty folder is left empty) and the error is returned

#### Scenario: Start a multi-package repository from nothing
- **WHEN** the user runs `gitpm init tilr/runtime --into repos/p --multi --no-release`
- **THEN** `repos/p` has `gitpm.json` and `packages/runtime`, one commit and no tag, and the suggested release command uses 0.1.0

#### Scenario: Tag fails when adding to an existing repository
- **WHEN** a package is added to an existing multi-package repository at version 1.2.0, with or without `--no-release`, and a tag command would fail
- **THEN** only the package addition is committed; no tag command runs or fails, and the suggested release command requires a new version

### Requirement: commit releases a version
`commit REPO VERSION [--package ID]` SHALL set every manifest version, stage all changes, commit `Release VERSION` and create one tag using `git tag -m MESSAGE`, honoring `tag.gpgSign` without an editor. It SHALL never push. REPO must be the root; a selector must match a package (`invalid_argument`). The selector only chooses the returned/printed ID; without it all packages are still released. Output SHALL be `ID committed VERSION (local commit and tag TAG)`.

#### Scenario: Invalid version
- **WHEN** the author runs `gitpm commit REPO 1.0` or `gitpm commit REPO 1.0.0+build`
- **THEN** it fails with `invalid_version` (format in package-ids) and nothing changes

#### Scenario: Wrong-typed metadata
- **WHEN** the manifest has `"tags": "ui"`
- **THEN** it fails with `invalid_manifest` (rule in plugin-metadata, "Lenient reading, strict authoring")

#### Scenario: Release one package of a multi-package repository
- **WHEN** the author runs `gitpm commit ./tilr-packages 0.2.0 --package tilr/store`
- **THEN** every package is set to 0.2.0, one commit and one tag `v0.2.0` are created, and it prints `tilr/store committed 0.2.0 (local commit and tag v0.2.0)`

#### Scenario: Release a pre-release
- **WHEN** the author runs `gitpm commit ./hello 1.3.0-beta.1`
- **THEN** it creates the tag `v1.3.0-beta.1`; consumers get it only when they ask for it (updates, "Pre-releases are opt-in")

#### Scenario: Signing configured without an editor
- **WHEN** `tag.gpgSign=true` and a usable signing key and Git identity are configured, and `commit` releases a repository with multiple packages
- **THEN** the single `vVERSION` tag is signed, its message is supplied with `-m`, and no editor is needed

#### Scenario: Release a multi-package repository without a selector
- **WHEN** the author runs `gitpm commit REPO 0.3.0` in a repository with several packages
- **THEN** all packages become 0.3.0 and share one release commit and one tag `v0.3.0`

### Requirement: Existing tags are never moved
`commit` SHALL refuse with `version_conflict` ("tag already exists: TAG") when the tag exists, and SHALL refuse reusing the current version (`version_conflict`, "choose a new version") unless it is a retry: the working tree is clean, every manifest at HEAD already has VERSION, and HEAD is either the release commit that set it or the initial repository creation commit made with `--no-release`; then only the tag is created.


#### Scenario: Release after init --no-release
- **WHEN** `init --no-release` made the commit `Create NAME 0.1.0` and the author runs `gitpm commit REPO 0.1.0`
- **THEN** only the tag `v0.1.0` is created

#### Scenario: Retry after a failed tag
- **WHEN** a previous `commit 0.2.0` created the release commit but the tag failed
- **THEN** running it again creates only the tag

#### Scenario: A package addition cannot reuse a release
- **WHEN** a package was added to a repository at 1.2.0, and the author runs `gitpm commit REPO 1.2.0`
- **THEN** it fails with `version_conflict`; the existing `v1.2.0` is never moved and a package addition commit is not a tag-only retry

#### Scenario: Retained upstream tag collides with a fork release
- **WHEN** a fork retained upstream tag `v1.2.0` and a release requests version 1.2.0
- **THEN** it fails with `version_conflict` naming `v1.2.0`, and the tag and repository are unchanged

### Requirement: Git identity is never invented in existing repositories
gitpm SHALL NOT write a Git identity into an existing repository; without a configured identity there (`user.useConfigOnly`), `commit` and `init` into an existing repository SHALL be refused with `invalid_argument`. Only a repository gitpm creates itself (new `init` folder, new fork) MAY get the local identity `Package Demo <demo@example.invalid>` when the user has none. Verified by `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`.

#### Scenario: Tag-only retry without identity
- **WHEN** an existing repository has no identity and `commit` is retried after a failed tag
- **THEN** the command fails with `invalid_argument` ("no Git identity configured; set user.name and user.email"), no tag is created and the repository is unchanged; annotated and signed tags require identity too

#### Scenario: No identity
- **WHEN** git cannot determine a user identity for an existing repository and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the command fails with `invalid_argument` ("no Git identity configured; set user.name and user.email") and the repository is unchanged
- **AND** an identity git would derive on its own from the login and host name is not accepted; `user.name`/`user.email` in any Git config, or the `GIT_AUTHOR_*`/`GIT_COMMITTER_*` variables, count; this is covered by `scaffold::derived_identity_is_refused`

### Requirement: commit returns what it released
`commit_package` SHALL return `{package, version, tag}` of the repository release it made; `package` is the selected package ID and does not restrict which packages are released (the tag as in package-ids, "Release tag names"), and the CLI SHALL print its line from that result without computing the tag itself. Verified by `scaffold::commit_package_returns_package_version_and_tag`.

#### Scenario: Result of a release
- **WHEN** the author runs `gitpm commit ./tilr-plugins 0.2.0 --package tilr/store`
- **THEN** the library returns package `tilr/store`, version `0.2.0` and tag `v0.2.0`, and the CLI prints `tilr/store committed 0.2.0 (local commit and tag v0.2.0)`
