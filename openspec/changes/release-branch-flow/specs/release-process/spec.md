## RENAMED Requirements

- FROM: `### Requirement: Specification change authorization on dev`
- TO: `### Requirement: Specification change authorization on main`

## MODIFIED Requirements

### Requirement: Branch flow
Work SHALL happen on task branches off `main`; changes SHALL reach `main` only through a pull request; Dependabot SHALL target `main`, the default branch (no `target-branch` in `.github/dependabot.yml`). `main` SHALL be protected (PR only, squash merges, linear history, required checks, no deletion or force-push), and tags `v*` SHALL NOT be deleted or moved. Nothing SHALL be released from `main`: on the owner's explicit instruction a release branch `release/X.Y` is cut from a commit of `main`, and fixes for it SHALL land in `main` first and then be cherry-picked into `release/X.Y`. These rules live in the GitHub rulesets (`main`, `release tags`; `gh api repos/OWNER/REPO/rulesets`) and `.github/dependabot.yml`.

#### Scenario: Merge without a target
- **WHEN** an agent is told to "merge" without naming a branch
- **THEN** it merges a pull request into `main`

#### Scenario: Dependabot update
- **WHEN** Dependabot proposes a cargo or GitHub Actions update
- **THEN** the pull request's base branch is `main`
- **AND** no update block of `.github/dependabot.yml` sets `target-branch`

#### Scenario: After a release merge
- **WHEN** the release PR is squash-merged into `release/0.6` and a later fix is needed for 0.6
- **THEN** nothing is merged from `release/0.6` back into `main`
- **AND** the fix is merged into `main` through a pull request first and then cherry-picked into `release/0.6`

### Requirement: Required CI checks
Every pull request into `main` or a release branch `release/**`, every merge queue group (`merge_group`) and every push to `main` or `release/**` SHALL run `fmt + clippy` (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`), `test` (`cargo test --workspace --locked` plus a `gitpm --version`/`--help` smoke test), `msrv (core crate)` (Rust 1.92, `cargo check -p gitpm -p gitpm-cli --all-targets --locked`), `cargo-deny` and `gitleaks`, with tools pinned by version and SHA-256 and actions pinned by commit.

#### Scenario: MSRV break
- **WHEN** a change uses an API newer than Rust 1.92 in `gitpm` or `gitpm-cli`
- **THEN** the `msrv (core crate)` check fails and the PR cannot merge

#### Scenario: Release PR is checked
- **WHEN** the release PR into `release/0.6` is opened or updated
- **THEN** the same checks run on it as on a pull request into `main`

### Requirement: Versioning by Conventional Commits
Pull request titles SHALL be Conventional Commits (they become the squash commit). release-please SHALL run only on pushes to release branches `release/**` and maintain there a release PR `chore(release/X.Y): release X.Y.Z` into the pushed branch (`target-branch`); a push to `main` SHALL NOT open or update a release PR. `fix`/`perf` bump patch (release-please defaults; `perf` is not configured here), `feat` bumps minor, and before 1.0 `feat!`/BREAKING also bumps minor (`bump-minor-pre-major` in `release-please-config.json`). The release PR SHALL be opened by the GitHub App `tilr-bot` so CI runs on it without bypass.

#### Scenario: Feature release
- **WHEN** `feat: …` is merged into `main` at version 0.5.0 and `release/0.6` is cut from that commit
- **THEN** the release PR into `release/0.6` proposes 0.6.0 with a CHANGELOG entry

#### Scenario: Push to main
- **WHEN** a pull request is merged into `main`
- **THEN** no release PR is opened or updated and nothing is published

### Requirement: Merging the release PR is the release
Merging the release PR into its release branch SHALL create tag `vX.Y.Z` and a GitHub Release, then publish `gitpm` and then `gitpm-cli` to crates.io through Trusted Publishing (OIDC, environment `crates-io` restricted to `release/*` branches, no long-lived tokens), skipping a version already on crates.io, after re-running fmt, clippy and tests on the tag. Releasing SHALL be a separate explicit step by the owner.

#### Scenario: Re-run after partial publish
- **WHEN** `gitpm` was published but `gitpm-cli` failed and the job is re-run
- **THEN** `gitpm` is skipped as already published and `gitpm-cli` is published

### Requirement: Specification change authorization on main
Pull requests into main SHALL pass the required spec-change check. One shared checker SHALL enforce intake Spec scope and accept authorization only from Dan's comments, using the dedicated read-only task credential. The check SHALL run only from the trusted pull_request_target workflow in main; a copy of that workflow in the pull request is never run. The other checks run in CI (Required CI checks).

#### Scenario: Existing specification is changed without authorization
- **WHEN** an intake-ready comment declares Spec scope and a pull request removes or changes existing specification lines without a matching spec-change entry from Dan
- **THEN** the check fails and lists the uncovered requirements

#### Scenario: Additions and the task's own change
- **WHEN** a pull request only adds requirements or edits its own change declared by intake
- **THEN** the check passes without an authorization block
- **AND** deltas that modify existing requirements still require authorization

#### Scenario: Authorized changes
- **WHEN** every affected existing requirement and foreign change has a matching operation in Dan's spec-change block for this component
- **THEN** the check passes

#### Scenario: Bot impersonates authorization
- **WHEN** an authorization block is posted by a bot instead of Dan
- **THEN** the block does not authorize any changes

#### Scenario: Task predates scope declarations
- **WHEN** the task's intake-ready comments contain no Spec scope line
- **THEN** the check passes as a legacy task

#### Scenario: Foreign change edited or archived
- **WHEN** a pull request edits or archives another task's change
- **THEN** the matching edit or archive entry from Dan is required

#### Scenario: Task data is unavailable
- **WHEN** the task cannot be identified or its comments cannot be read completely
- **THEN** the check fails with an actionable error

#### Scenario: Local merge automation
- **WHEN** local merge automation invokes the checker with a repository, base, head, component, and task branch
- **THEN** the same authorization rules and exit status apply as in CI

#### Scenario: Candidate code cannot replace the checker
- **WHEN** a pull request changes checker scripts or workflow definitions
- **THEN** the check still runs the workflow from main, not a workflow file from the pull request
- **AND** the trusted workflow reads candidate Git objects without checking out or executing candidate files
- **AND** the shared checker content is verified against a digest pinned in main's workflow before execution, and a new digest takes effect only through a pull request into main
- **AND** only the checker process receives the read-only task credential

#### Scenario: Malformed scope declaration
- **WHEN** the latest intake-ready Spec scope line is empty or does not identify an own change
- **THEN** the check fails with an error instead of accepting the task as legacy

#### Scenario: A renamed requirement is also modified in a delta
- **WHEN** an own change renames an existing requirement and modifies it under the new name
- **THEN** authorization to rename the original requirement does not authorize modifying it
- **AND** modification authorization is checked against the original requirement name

#### Scenario: A foreign change is rewritten while archived
- **WHEN** a foreign change is archived with altered file contents
- **THEN** both archive and edit authorization are required for that change

#### Scenario: Text is inserted into an existing requirement
- **WHEN** a pull request adds a sentence, scenario, or scenario step inside a requirement already present in main
- **THEN** matching modify authorization for that existing requirement is required
- **AND** adding a separate new requirement remains allowed without authorization

#### Scenario: An active foreign change is edited while copied to archive
- **WHEN** a foreign change is copied unchanged to archive while its active source is modified
- **THEN** both archive and edit authorization are required for that change

#### Scenario: A previous foreign archive is changed while another is created
- **WHEN** a pull request edits or removes an existing foreign archive and creates a new archive copy of the same change
- **THEN** archive authorization for the new copy does not authorize editing the previous archive

#### Scenario: Chained requirement renames retain their original authorization
- **WHEN** an own delta renames an existing requirement through intermediate names and modifies or removes it
- **THEN** the operation requires authorization for the original requirement in main
- **AND** cyclic or ambiguous rename mappings fail the check

#### Scenario: A new requirement takes an existing scenario
- **WHEN** an inserted requirement heading moves existing scenario lines out of a requirement that remains in the specification
- **THEN** the check requires modify authorization for that existing requirement

#### Scenario: Reusing a renamed requirement's former name
- **WHEN** an own delta renames an existing requirement and another requirement takes its former name
- **THEN** each rename remains authorized against its own original requirement in main
- **AND** modifying or removing the first requirement under its new name requires permission for that original requirement

#### Scenario: A requirement's text moves under another existing requirement
- **WHEN** a pull request deletes or moves the heading of an existing requirement so that some of its text or scenarios fall under another requirement already present in main
- **THEN** authorization to remove or modify the requirement whose heading changed does not cover the change
- **AND** modify authorization for the requirement that absorbs the text is required

#### Scenario: Scope declaration names its own change ambiguously
- **WHEN** the latest intake-ready Spec scope line names no change or more than one change inside its own(...) part, or names a change outside it
- **THEN** the check fails with an error instead of treating any other word as the own change

#### Scenario: An existing archive shares the own change name
- **WHEN** a pull request edits an archive that already exists in main under the same change name as the task's own change
- **THEN** the archive is treated as a foreign change and edit authorization is required

#### Scenario: A new capability differs from an existing one only by letter case or Unicode form
- **WHEN** a pull request adds a capability directory whose name equals an existing or another added capability after case folding and Unicode normalization
- **THEN** the check fails with an error instead of treating it as a new capability
- **AND** the same applies when a delta in the task's own change names a capability that differs from an existing one only by letter case or Unicode form

### Requirement: Specification merge readiness diagnostics
Before fetching a pull request test merge for specification authorization, the trusted check SHALL read the pull request through the GitHub API with pull-requests read permission. It SHALL distinguish a conflict with main from a pending or stale merge result, and SHALL inspect candidate Git objects without executing candidate code.

#### Scenario: Pull request conflicts with dev
- **WHEN** the API reports mergeable false for a head SHA matching the requested candidate SHA
- **THEN** the check fails immediately with a conflict diagnostic naming the task branch and instructing the author to merge main with `git merge main` and push
- **AND** it does not advise re-running the job

#### Scenario: GitHub is still computing the merge
- **WHEN** mergeable is null or the fetched merge commit does not have the requested candidate SHA as its second parent
- **THEN** the check waits and reads the API again for up to approximately 60 seconds
- **AND** it proceeds automatically once the API head SHA matches the requested candidate SHA, mergeable is true and the merge commit has the requested candidate SHA as its second parent
- **AND** it uses the first parent of that verified merge as the comparison baseline

#### Scenario: API response describes a stale head
- **WHEN** the API head SHA differs from the requested candidate SHA
- **THEN** the check treats that response as stale and reads the API again within the existing deadline
- **AND** it does not interpret its mergeable value as the candidate's conflict or readiness

#### Scenario: Command fails
- **WHEN** the API or Git command fails or times out
- **THEN** the check fails with a diagnostic identifying the command and preserving its error output

#### Scenario: Merge result remains stale
- **WHEN** the merge result remains pending or stale until the wait deadline
- **THEN** the check fails with the diagnostic that GitHub has not updated the merge result and advises re-running the job

#### Scenario: Deterministic regression coverage
- **WHEN** merge readiness decisions are tested
- **THEN** API responses, fetched parents, elapsed time and waits are controlled by fixtures without real waiting
- **AND** coverage includes a pending result becoming current, an immediate conflict, a stale API head becoming current, and stale results reaching the deadline
