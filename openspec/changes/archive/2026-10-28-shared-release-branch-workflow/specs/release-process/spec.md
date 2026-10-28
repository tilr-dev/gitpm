## ADDED Requirements

### Requirement: Shared release branch orchestration
The release workflow SHALL delegate release-line version selection and release candidate tagging to `tilr-dev/.github/.github/workflows/release-branch.yml`. Callers SHALL pin the workflow to an immutable commit and retain artifact builds and publication in their own repositories.

#### Scenario: A release branch is pushed
- **WHEN** a push to `release/X.Y` creates no stable release
- **THEN** release-please opens or updates its PR targeting the pushed branch using the tilr-bot token
- **AND** the pushed commit receives `vX.Y.Z-rc.N`, with the next stable patch of the line and the next candidate number
- **AND** the caller builds the candidate and publishes a GitHub prerelease with the same target archives and checksums as a stable release

#### Scenario: Another push and a rerun
- **WHEN** another commit is pushed on the same release line
- **THEN** it receives the next candidate number
- **AND** rerunning a commit already tagged as a candidate reuses its tag

#### Scenario: A stable release is created
- **WHEN** the release PR is merged and release-please creates a stable release
- **THEN** no candidate is tagged and the caller retains its stable publication behavior

#### Scenario: Invalid branch
- **WHEN** a branch named `release/next` invokes the workflow
- **THEN** it fails with `release branches are named release/X.Y` before opening a PR, tagging or building

#### Scenario: A stable commit is rerun
- **WHEN** the pushed commit already has a stable tag on its line
- **THEN** no new candidate or candidate build is produced

#### Scenario: Candidate binaries identify the selected version
- **WHEN** a candidate tag is built
- **THEN** `gitpm --version` prints the tag version selected by the shared workflow
- **AND** crates.io publication remains restricted to newly created stable releases
