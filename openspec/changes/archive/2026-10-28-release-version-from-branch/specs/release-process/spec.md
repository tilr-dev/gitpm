## MODIFIED Requirements

### Requirement: Versioning by Conventional Commits
Pull request titles SHALL be Conventional Commits (they become the squash commit). release-please SHALL run only on pushes to release branches `release/**` and maintain there a release PR `chore(release/X.Y): release X.Y.Z` into the pushed branch (`target-branch`), opened by the GitHub App `tilr-bot` so CI runs on it without bypass; a push to `main` SHALL NOT open or update a release PR.

#### Scenario: Version bumps
- **WHEN** release-please computes the next version
- **THEN** the branch `release/X.Y` selects `X.Y.0` until a stable `vX.Y.*` exists, then the highest stable patch plus one
- **AND** Conventional Commits select releasable changes and changelog sections; they SHALL NOT move the version outside the branch line, including breaking changes

#### Scenario: Feature release
- **WHEN** `feat: …` is merged into `main` at version 0.5.0 and `release/0.6` is cut from that commit
- **THEN** the release PR into `release/0.6` proposes 0.6.0 with a CHANGELOG entry

#### Scenario: Push to main
- **WHEN** a pull request is merged into `main`
- **THEN** no release PR is opened or updated and nothing is published

#### Scenario: A new release branch follows another line
- **GIVEN** `v0.1.0` exists on `release/0.1` and its release commit is absent from `main`
- **WHEN** a new feature lands in `main` and `release/0.2` is cut from it
- **THEN** the release PR proposes 0.2.0 and its changelog contains the new feature only, excluding changes already shipped in 0.1.0

#### Scenario: A new line contains only a fix
- **GIVEN** the inherited manifest says 0.2.1
- **WHEN** only a `fix:` commit is added and `release/0.3` is cut
- **THEN** the release PR proposes 0.3.0

#### Scenario: A feature on an existing line
- **GIVEN** stable `v0.3.2` exists in the history of `release/0.3`
- **WHEN** a new `feat:` commit is pushed to that branch
- **THEN** the release PR proposes 0.3.3, even if another line has a newer stable release

#### Scenario: A fix already shipped by cherry-pick
- **GIVEN** a fix landed in `main` and its cherry-pick shipped on the previous release line
- **WHEN** the next release branch is cut from `main`
- **THEN** the changelog excludes shared history and that patch-equivalent fix

#### Scenario: Candidate tags do not select stable versions
- **GIVEN** candidate tags exist on a release line
- **WHEN** the runner selects its next version and changelog baseline
- **THEN** only stable tags are considered: the highest stable tag of the same line, otherwise the highest earlier stable version
- **AND** the first release is `X.Y.0`, and subsequent releases increment the highest stable patch

#### Scenario: A stable tag is outside the same-line history
- **GIVEN** a stable tag exists for the target release line outside the branch history
- **WHEN** the runner prepares a release
- **THEN** it fails before creating a release PR or publishing

#### Scenario: No earlier stable release exists
- **GIVEN** no earlier stable tag exists
- **WHEN** the runner prepares the first release
- **THEN** the configured bootstrap SHA bounds the changelog history

#### Scenario: Runtime overrides remain local
- **WHEN** the runner selects a version and changelog baseline
- **THEN** it copies no release commits or version files into `main`
