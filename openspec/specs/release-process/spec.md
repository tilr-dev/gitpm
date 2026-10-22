# release-process Specification

## Purpose

Defines how gitpm itself is developed and released: the dev/main branch flow, required CI checks, release-please versioning, lockstep crates, crates.io Trusted Publishing and prebuilt binaries.

## Requirements

### Requirement: Branch flow
Work SHALL happen on `dev` or feature branches off `dev`; changes SHALL reach `main` only through a pull request (from `dev`, or the release PR) on the owner's explicit instruction; Dependabot SHALL target `dev`. `main` SHALL be protected (PR only, squash merges, linear history, required checks, no deletion or force-push), and tags `v*` SHALL NOT be deleted or moved. These rules live in the GitHub rulesets (`main`, `release tags`; `gh api repos/OWNER/REPO/rulesets`) and `.github/dependabot.yml`.

#### Scenario: Merge without a target
- **WHEN** an agent is told to "merge" without naming a branch
- **THEN** it merges into `dev`, not `main`

#### Scenario: Dependabot update
- **WHEN** Dependabot proposes a cargo or GitHub Actions update
- **THEN** the pull request's base branch is `dev`
- **AND** every update block of `.github/dependabot.yml` sets `target-branch: dev`

#### Scenario: After a release merge
- **WHEN** the release PR `chore(main): release X.Y.Z` is squash-merged into `main`
- **THEN** `dev` is reset or fast-forwarded to `main` before the next change lands on it
- **AND** so the next pull request from `dev` carries only new commits

### Requirement: Required CI checks
Every pull request and every push to `main` SHALL run `fmt + clippy` (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`), `test` (`cargo test --workspace --locked` plus a `gitpm --version`/`--help` smoke test), `msrv (core crate)` (Rust 1.92, `cargo check -p gitpm -p gitpm-cli --all-targets --locked`), `cargo-deny` and `gitleaks`, with tools pinned by version and SHA-256 and actions pinned by commit.

#### Scenario: MSRV break
- **WHEN** a change uses an API newer than Rust 1.92 in `gitpm` or `gitpm-cli`
- **THEN** the `msrv (core crate)` check fails and the PR cannot merge

### Requirement: Versioning by Conventional Commits
Pull request titles SHALL be Conventional Commits (they become the squash commit). release-please SHALL maintain a release PR `chore(main): release X.Y.Z` from `main`: `fix`/`perf` bump patch (release-please defaults; `perf` is not configured here), `feat` bumps minor, and before 1.0 `feat!`/BREAKING also bumps minor (`bump-minor-pre-major` in `release-please-config.json`). The release PR SHALL be opened by the GitHub App `tilr-bot` so CI runs on it without bypass.

#### Scenario: Feature release
- **WHEN** `feat: …` is merged into `main` at version 0.5.0
- **THEN** the release PR proposes 0.6.0 with a CHANGELOG entry

### Requirement: Lockstep crates
`gitpm` (the release-please package), `gitpm-cli` and `gitpm-ffi` SHALL share one version: release-please SHALL update the versions of `gitpm-cli` and `gitpm-ffi`, their `gitpm` dependency versions and the `Cargo.lock` entries. `gitpm-ffi` is versioned but not released (ffi-bindings, "FFI is not published").

#### Scenario: Release PR contents
- **WHEN** the release PR for 0.6.0 is opened
- **THEN** `crates/gitpm/Cargo.toml`, `crates/gitpm-cli/Cargo.toml` and `crates/gitpm-ffi/Cargo.toml` say 0.6.0

### Requirement: Merging the release PR is the release
Merging the release PR SHALL create tag `vX.Y.Z` and a GitHub Release, then publish `gitpm` and then `gitpm-cli` to crates.io through Trusted Publishing (OIDC, environment `crates-io` restricted to `main`, no long-lived tokens), skipping a version already on crates.io, after re-running fmt, clippy and tests on the tag. Releasing SHALL be a separate explicit step by the owner.

#### Scenario: Re-run after partial publish
- **WHEN** `gitpm` was published but `gitpm-cli` failed and the job is re-run
- **THEN** `gitpm` is skipped as already published and `gitpm-cli` is published

### Requirement: Prebuilt binaries
Each release SHALL attach `gitpm-cli-vX.Y.Z-TARGET.tar.gz` and a `.sha256` for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (glibc 2.35+), holding the `gitpm` binary, licenses and README, with names matching the cargo-binstall metadata. Native builds SHALL check `gitpm --version` equals the tag.

#### Scenario: cargo binstall
- **WHEN** a user runs `cargo binstall gitpm-cli`
- **THEN** the prebuilt archive of their target is downloaded from the GitHub Release

### Requirement: Scoped security-check exceptions
Security-check exceptions SHALL identify a specific confirmed finding and document
its reason. A synthetic credential used only by a local test server MAY be ignored
by its gitleaks fingerprint. An unmaintained transitive dependency MAY have an
advisory-specific exception when no released upstream update removes it; the
exception SHALL document a dated verification and a removal condition. Other
secrets and advisories SHALL remain subject to the existing checks.

#### Scenario: Loopback Git fixture in history
- **WHEN** gitleaks scans the test-only credential introduced in commit bbe8cf6511b9cb96d858f7c65f046e0c06dc9295 at itests/scenarios/uc_049.py:25
- **THEN** only that finding's fingerprint is ignored
- **AND** unrelated credentials are still reported

#### Scenario: Unmaintained transitive parser without an upstream replacement
- **WHEN** the released UniFFI and Askama dependency tree still requires basic-toml
- **THEN** cargo-deny permits only advisory RUSTSEC-2026-0374 with a documented reason
- **AND** all other advisory checks stay enabled
- **AND** the exception is removed once an upstream update removes basic-toml
