## MODIFIED Requirements

### Requirement: init creates a package with its first release
`init NAMESPACE/NAME --into DIR` SHALL scaffold a package with a manifest (`id`, `version` 0.1.0, `"title": "NAME"`), a demo `dist/index.js`, one commit and the tag `NAMESPACE/NAME@0.1.0`. A `DIR` that does not exist SHALL become a single-package repository. `--into` is required and nothing SHALL ever be overwritten. Verified by `scaffold::init_into_a_new_folder_makes_a_tagged_single_package_repository` and `scaffold::init_never_overwrites_and_validates_ids`.

#### Scenario: Scaffold files and commit
- **WHEN** `init acme/hello --into hello` creates a new single-package repository
- **THEN** it also writes `.gitignore` and `README.md`, and the one commit is `Create hello 0.1.0` (a package added to a multi-package repository gets `Create acme/hello 0.1.0`)

#### Scenario: New package repository
- **WHEN** the user runs `gitpm init acme/hello --into hello`
- **THEN** `hello` is a Git repository at tag `acme/hello@0.1.0` and the output contains `Created:`, `ID: acme/hello` and `Version 0.1.0: Git commit and tag acme/hello@0.1.0.`

#### Scenario: init without --into
- **WHEN** the user runs `gitpm init acme/hello`
- **THEN** it fails with `init needs --into DIR: …` and creates nothing

#### Scenario: Former alias
- **WHEN** the user runs `gitpm create acme/hello --into hello`
- **THEN** it fails with `unknown command: create; use --help` and creates nothing

### Requirement: commit releases a version
`commit REPO VERSION [--package ID]` SHALL set `version` in the package's manifest, stage ALL changes of the repository, commit `Release ID VERSION` and create the release tag (package-ids, "Release tag names"), printing `ID committed VERSION (local commit and tag TAG)`. It SHALL never push. REPO MUST be the repository root and a given `--package` MUST match a package (both `invalid_argument`). Verified by `scaffold::commit_bumps_version_commits_everything_and_tags`.

#### Scenario: Invalid version
- **WHEN** the author runs `gitpm commit REPO 1.0`
- **THEN** it fails with `invalid_version` (format in package-ids) and nothing changes

#### Scenario: Wrong-typed metadata
- **WHEN** the manifest has `"tags": "ui"`
- **THEN** it fails with `invalid_manifest` (rule in plugin-metadata, "Lenient reading, strict authoring")

#### Scenario: Release one package of a multi-package repository
- **WHEN** the author runs `gitpm commit ./tilr-packages 0.2.0 --package tilr/store`
- **THEN** it prints `tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)`

### Requirement: remote sets origin without network
`remote REPO URL` SHALL add or replace `origin` (all configured URLs, fetch and push) of a package repository root without network access; URLs that are empty, start with `-` or contain control characters SHALL be refused with `invalid_argument`. Verified by `scaffold::set_remote_adds_or_replaces_origin_without_network`.

#### Scenario: Not a package repository root
- **WHEN** REPO is a subfolder (`invalid_argument`, "pass the package repository root") or a repository root without a manifest and without `gitpm.json`
- **THEN** the command fails (`invalid_manifest` for the latter) and nothing changes

#### Scenario: Set origin
- **WHEN** the user runs `gitpm remote ./hello git@github.com:me/hello.git`
- **THEN** it prints `Origin for ./hello set to git@github.com:me/hello.git (fetch and push).`

#### Scenario: Other remotes are kept
- **WHEN** `origin` has a separate push URL, the repository has another remote `backup`, and the user runs `gitpm remote REPO URL`
- **THEN** the fetch and push URLs of `origin` are both `URL`, `backup` is unchanged and nothing is pushed to any remote

### Requirement: Git identity is never invented in existing repositories
gitpm SHALL NOT write a Git identity into an existing repository; when git cannot determine an author and committer identity there, `commit`, `init` into an existing repository and adding packages to an existing fork SHALL be refused with `git`. Only a repository gitpm creates itself (a new `init` folder, a new fork) MAY get a local demo identity `Package Demo <demo@example.invalid>` when the user has none. Verified by `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`.

#### Scenario: Tag-only retry without identity
- **WHEN** an existing repository has no identity and `commit` is retried after a failed tag
- **THEN** the identity is not checked and the lightweight tag is created

#### Scenario: No identity
- **WHEN** git cannot determine a user identity for an existing repository and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the command fails with `git` and the repository is unchanged
- **AND** the refusal is the error of `git var GIT_AUTHOR_IDENT` / `GIT_COMMITTER_IDENT`, so an identity git can derive on its own is accepted
