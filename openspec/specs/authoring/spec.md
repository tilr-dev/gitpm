# authoring Specification

## Purpose

Defines the author-side commands that create package repositories and releases: `init`/`create`, `commit` (version bump, commit, tag), `remote`, and the rules that keep releases immutable and author repositories intact. gitpm never pushes or uploads.

## Requirements

### Requirement: init creates a package with its first release
`init NAMESPACE/NAME --into DIR` (alias `create`) SHALL scaffold a package with `plugin.json` (`id`, `version` 0.1.0, `"title": "NAME"`), a demo `dist/index.js`, one commit and the tag `NAMESPACE/NAME@0.1.0`. A `DIR` that does not exist SHALL become a single-package repository. `--into` is required and nothing SHALL ever be overwritten. Verified by `scaffold::init_into_a_new_folder_makes_a_tagged_single_package_repository` and `scaffold::init_never_overwrites_and_validates_ids`.

#### Scenario: Scaffold files and commit
- **WHEN** `init acme/hello --into hello` creates a new single-package repository
- **THEN** it also writes `.gitignore` and `README.md`, and the one commit is `Create hello 0.1.0` (a package added to a multi-package repository gets `Create acme/hello 0.1.0`)

#### Scenario: New package repository
- **WHEN** the user runs `gitpm init acme/hello --into hello`
- **THEN** `hello` is a Git repository at tag `acme/hello@0.1.0` and the output contains `Created:`, `ID: acme/hello` and `Version 0.1.0: Git commit and tag acme/hello@0.1.0.`

#### Scenario: init without --into
- **WHEN** the user runs `gitpm init acme/hello`
- **THEN** it fails with `init needs --into DIR: …` and creates nothing

### Requirement: init into a multi-package repository
For a `DIR` with `gitpm.json`, init SHALL add the package at `PREFIX/NAME` for the first `PREFIX/*` pattern (else `packages/NAME`, appended to `gitpm.json` as a literal folder), committing only `gitpm.json` and the new folder; `gitpm.json` MUST have no uncommitted changes. A repository without `gitpm.json` and without a root package SHALL get `{"packages": ["packages/*"]}`. Every refusal below SHALL be `invalid_argument` and change nothing.

#### Scenario: Root is a package
- **WHEN** the repository root has `plugin.json` and no `gitpm.json`
- **THEN** init is refused

#### Scenario: Existing folder that is not a repository
- **WHEN** `DIR` exists and is not a Git repository (an empty folder without `--multi`)
- **THEN** init is refused with `… exists but is not a Git repository; pass a new folder or a repository`

#### Scenario: Not the repository root
- **WHEN** `DIR` is a subfolder of a repository
- **THEN** init is refused with `pass the repository root`

#### Scenario: ID differing only in case
- **WHEN** the repository has `a/ui` and the user runs `init A/ui --into REPO`
- **THEN** init is refused (`package a/ui already exists in this repository`)

#### Scenario: Second package with the same name
- **WHEN** `a/ui` exists in the repository and the user runs `init b/ui --into REPO`
- **THEN** it is refused with `invalid_argument` naming the package that has the folder
- **AND** this is covered by `scaffold::packages_are_added_to_a_multi_package_repository_and_released_by_id` and `scaffold::new_packages_follow_the_first_folder_pattern`

### Requirement: init options
`init --multi` SHALL make a new or empty folder a multi-package repository (`gitpm.json` and `packages/NAME`); `init --no-release` SHALL commit without the release tag and print the `gitpm commit … 0.1.0` command to release later. When gitpm adds a package to an existing repository and only the tag fails, the commit SHALL stay and the `git` error SHALL name the tag. A repository gitpm created itself SHALL be removed again on any failure, including the tag.

#### Scenario: Failure in a repository gitpm created
- **WHEN** the tag fails during a single-package `init` or `init --multi` into a new or empty folder
- **THEN** the new repository is removed (an adopted empty folder is left empty) and the error is returned

#### Scenario: Start a monorepo from nothing
- **WHEN** the user runs `gitpm init tilr/runtime --into repos/p --multi --no-release`
- **THEN** `repos/p` has `gitpm.json` and `packages/runtime`, one commit and no tag
- **AND** this is covered by `init_multi_creates_a_multi_package_repository_that_takes_more_packages` and `init_no_release_commits_without_a_tag`

#### Scenario: Tag fails when adding to an existing repository
- **WHEN** a package is added to an existing multi-package repository and creating its tag fails
- **THEN** the commit stays, the error kind is `git` and the message says `git tag ID@0.1.0`
- **AND** this is covered by `scaffold::create_package_keeps_the_commit_when_only_the_tag_fails`

### Requirement: commit releases a version
`commit REPO VERSION [--package ID]` SHALL set `version` in the package's manifest, stage ALL changes of the repository, commit `Release ID VERSION` and tag `ID@VERSION` (`vVERSION` for legacy), printing `ID committed VERSION (local commit and tag TAG)`. It SHALL never push. REPO MUST be the repository root (`invalid_argument`) and a given `--package` MUST match a package. Verified by `scaffold::commit_bumps_version_commits_everything_and_tags`.

#### Scenario: Invalid version
- **WHEN** the author runs `gitpm commit REPO 1.0`
- **THEN** it fails with `invalid_version` (format in package-ids) and nothing changes

#### Scenario: Wrong-typed metadata
- **WHEN** the manifest has `"tags": "ui"`
- **THEN** it fails with `invalid_manifest` (rule in plugin-metadata, "Lenient reading, strict authoring")

#### Scenario: Release one package of a monorepo
- **WHEN** the author runs `gitpm commit ./tilr-plugins 0.2.0 --package tilr/store`
- **THEN** it prints `tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)`

### Requirement: Existing tags are never moved
`commit` SHALL refuse with `version_conflict` ("tag already exists: TAG") when the tag exists, and SHALL refuse reusing the current version (`version_conflict`, "choose a new version") unless it is a retry: the working tree is clean, the manifest at HEAD already has VERSION and HEAD is the commit that set it; then only the tag is created. Verified by `scaffold::commit_can_be_retried_after_a_failed_tag` and `scaffold::commit_refusals_leave_the_repository_untouched`.

#### Scenario: Release after init --no-release
- **WHEN** `init --no-release` made the commit `Create NAME 0.1.0` and the author runs `gitpm commit REPO 0.1.0`
- **THEN** only the tag `ID@0.1.0` is created

#### Scenario: Retry after a failed tag
- **WHEN** a previous `commit 0.2.0` created the release commit but the tag failed
- **THEN** running it again creates only the tag

### Requirement: Failed commits restore the repository
When `git commit` fails (hook, signing), gitpm SHALL restore `plugin.json` and the Git index as they were and fail with `git`. Verified by `scaffold::failed_commit_restores_manifest_and_index`.

#### Scenario: Rejecting pre-commit hook
- **WHEN** a pre-commit hook rejects the release commit
- **THEN** `plugin.json` keeps its old version and the index is unchanged

### Requirement: Git identity is never invented in existing repositories
gitpm SHALL NOT write a Git identity into an existing repository; without a configured identity `commit` and adding packages SHALL be refused. Only a repository gitpm creates itself (a new `init` folder, a new fork) MAY get a local demo identity `Plugin Demo <demo@example.invalid>` when the user has none. Verified by `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`.

#### Scenario: Tag-only retry without identity
- **WHEN** an existing repository has no identity and `commit` is retried after a failed tag
- **THEN** the identity is not checked and the lightweight tag is created

#### Scenario: No identity
- **WHEN** an existing repository has no user identity configured and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the command fails and the repository is unchanged

### Requirement: remote sets origin without network
`remote REPO URL` SHALL add or replace `origin` (all configured URLs, fetch and push) of a package repository root without network access; URLs that are empty, start with `-` or contain control characters SHALL be refused with `invalid_argument`. Verified by `scaffold::set_remote_adds_or_replaces_origin_without_network`.

#### Scenario: Not a package repository root
- **WHEN** REPO is a subfolder (`invalid_argument`, "pass the plugin repository root") or a root without `plugin.json` and `gitpm.json`
- **THEN** the command fails (`invalid_manifest` for the latter) and nothing changes

#### Scenario: Set origin
- **WHEN** the user runs `gitpm remote ./hello git@github.com:me/hello.git`
- **THEN** it prints `Origin for ./hello set to git@github.com:me/hello.git (fetch and push).`
