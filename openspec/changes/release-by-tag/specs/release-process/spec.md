## MODIFIED Requirements

### Requirement: Branch flow
Changes SHALL reach `main` only through a pull request from a task branch off `main`, and nothing SHALL be released from `main`: on the owner's explicit instruction a release branch `release/X.Y` is cut from a commit of `main`, and its fixes SHALL land in `main` first and then be cherry-picked. These rules live in the GitHub rulesets `main`, `release branches` and `release tags` (`gh api repos/OWNER/REPO/rulesets`) and in `.github/dependabot.yml`.

#### Scenario: Protected branches and tags
- **WHEN** someone pushes directly to `main`, force-pushes it or deletes it
- **THEN** the `main` ruleset refuses it: `main` changes only through squash pull requests with linear history and required checks
- **AND** tags `v*` cannot be deleted or moved

#### Scenario: Release branch protection
- **WHEN** someone who is not a repository admin creates `release/0.6` or pushes to it directly
- **THEN** the `release branches` ruleset refuses it: release branches change only through squash pull requests with linear history and are never deleted or force-pushed
- **AND** repository admins may bypass that ruleset, and CI checks on release branches are not required

#### Scenario: Merge without a target
- **WHEN** an agent is told to "merge" without naming a branch
- **THEN** it merges a pull request into `main`

#### Scenario: Dependabot update
- **WHEN** Dependabot proposes a cargo or GitHub Actions update
- **THEN** the pull request's base branch is `main`
- **AND** no update block of `.github/dependabot.yml` sets `target-branch`

#### Scenario: After a release tag
- **WHEN** stable `v0.6.0` is tagged from `release/0.6` and a later fix is needed for 0.6
- **THEN** nothing is merged from `release/0.6` back into `main`
- **AND** the fix is merged into `main` through a pull request first and then cherry-picked into `release/0.6`

### Requirement: Required CI checks
Every pull request into `main` or `release/**`, every merge queue group (`merge_group`) and every push to `main` or `release/**` SHALL run the checks `fmt + clippy`, `test`, `msrv (core crate)`, `cargo-deny` and `gitleaks`, with tools pinned by version and SHA-256 and actions pinned by commit.

#### Scenario: Check commands
- **WHEN** the checks run
- **THEN** `fmt + clippy` runs `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
- **AND** `test` runs `cargo test --workspace --locked` plus a `gitpm --version`/`--help` smoke test
- **AND** `msrv (core crate)` runs `cargo check -p gitpm -p gitpm-cli --all-targets --locked` with Rust 1.92

#### Scenario: MSRV break
- **WHEN** a change uses an API newer than Rust 1.92 in `gitpm` or `gitpm-cli`
- **THEN** the `msrv (core crate)` check fails and the PR cannot merge

#### Scenario: Release branch fix is checked
- **WHEN** a pull request into `release/0.6` is opened or updated
- **THEN** the same checks run on it as on a pull request into `main`

### Requirement: Lockstep crates
`gitpm`, `gitpm-cli` and `gitpm-ffi` SHALL share one version. CI SHALL set their manifest versions, local `gitpm` dependency versions and workspace lockfile entries from the selected tag before publication or artifact builds, without committing version changes. `gitpm-ffi` SHALL remain unpublished.

#### Scenario: Tag overrides inherited manifests
- **GIVEN** checked-in crate versions differ from the selected tag
- **WHEN** CI prepares stable `v0.6.0` or candidate `v0.6.0-rc.1`
- **THEN** all three crate versions and local dependency and lockfile versions match the tag without its leading `v`
- **AND** locked builds use these versions without changing unrelated dependencies

### Requirement: Shared release branch orchestration
The release workflow SHALL delegate release-line version selection, tag validation and GitHub Release creation to `tilr-dev/.github/.github/workflows/release-branch.yml`, pinned by the same immutable commit in `uses` and `workflow-sha`. Callers SHALL retain artifact builds and publication, serialize release runs without cancellation, and trigger on `release/**` branch pushes and `v*` tag pushes excluding `v*-rc.*`.

#### Scenario: A release branch is pushed
- **WHEN** a new commit is pushed to `release/X.Y`
- **THEN** the shared workflow selects `X.Y.0` if the line has no stable tag, otherwise the highest stable patch plus one, and creates `vX.Y.Z-rc.N` with the next candidate number
- **AND** it creates a GitHub prerelease and the caller uploads CLI and Apple archives with checksums
- **AND** crates.io is unchanged and no release PR or generated CHANGELOG is maintained

#### Scenario: Another push and a rerun
- **WHEN** another commit is pushed on the same release line
- **THEN** it receives the next candidate number
- **AND** rerunning a commit already tagged as a candidate reuses its tag

#### Scenario: Invalid branch
- **WHEN** a branch named `release/next` invokes the workflow
- **THEN** it fails before tagging, building or publishing

#### Scenario: A stable commit is rerun
- **WHEN** the pushed branch commit already has a stable tag on its line
- **THEN** no new candidate or candidate build is produced

#### Scenario: Candidate binaries identify the selected version
- **WHEN** a candidate tag is built
- **THEN** `gitpm --version` prints the selected version including its RC suffix
- **AND** crates.io publication is skipped

#### Scenario: Candidate tag does not re-enter the caller
- **WHEN** tilr-bot pushes an RC tag
- **THEN** the tag trigger excludes it

## REMOVED Requirements

### Requirement: Versioning by Conventional Commits
**Reason**: Release versions are selected by tags, independently of commit titles.
**Migration**: Use the shared release workflow and owner-created stable tags.

### Requirement: Merging the release PR is the release
**Reason**: A stable tag is the explicit release approval.
**Migration**: Use the new A release tag is the release requirement.

## ADDED Requirements

### Requirement: A release tag is the release
A stable tag `vX.Y.Z` created by the owner through `bin/release` SHALL trigger a GitHub Release and publication of `gitpm` followed by `gitpm-cli`. The shared workflow MUST validate that the tagged commit belongs to the matching `release/X.Y` branch before publication. The tag SHALL select the version independently of commit types; pull request titles SHALL NOT require Conventional Commits.

#### Scenario: Stable tag on its release line
- **WHEN** `v0.6.0` is pushed from a commit contained in `release/0.6`
- **THEN** CI sets versions from the tag and reruns fmt, clippy and tests before publishing
- **AND** it publishes version 0.6.0 through crates.io Trusted Publishing (OIDC, environment `crates-io` permitting `v*` tags, no long-lived tokens)
- **AND** it uploads CLI and Apple artifacts to its GitHub Release

#### Scenario: Stable tag outside its release line
- **WHEN** a stable tag points outside the matching release branch or that branch is absent
- **THEN** release validation fails before crates.io publication or artifact upload

#### Scenario: Re-run after partial publish
- **WHEN** `gitpm` was published but `gitpm-cli` failed and the job is re-run
- **THEN** `gitpm` is skipped as already published and `gitpm-cli` is published

#### Scenario: Push to main
- **WHEN** a pull request is merged into `main`
- **THEN** no release is created and nothing is published
