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

The release version SHALL be `X.Y.0` for the first stable release of `release/X.Y`, then the highest stable `vX.Y.Z` plus one patch. Candidate tags SHALL NOT select the version or baseline. The changelog SHALL include only changes not shipped in the highest stable tag of that line, or the highest earlier stable version when this line has none. The runner SHALL exclude shared history and patch-equivalent cherry-picked changes. An existing same-line stable tag outside the branch history SHALL fail safely. With no earlier stable tag, the configured bootstrap SHA SHALL bound the history. The runtime overrides SHALL NOT copy release commits or version files into `main`.

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
- **THEN** the changelog excludes that patch-equivalent fix
