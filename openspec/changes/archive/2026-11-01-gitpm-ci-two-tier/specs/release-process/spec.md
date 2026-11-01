## MODIFIED Requirements

### Requirement: Required CI checks
Every pull request into `main` or `release/**`, every merge queue group (`merge_group`) and every push to `release/**` SHALL run the checks `fmt + clippy`, `test`, `msrv (core crate)`, `cargo-deny` and `gitleaks`, with tools pinned by version and SHA-256 and actions pinned by commit. A push to `main` SHALL NOT run them: the merge queue has already checked the commit that lands there. The check names SHALL be the same on every event; only the depth of `test` depends on the event: a pull request into `main` runs the fast tier, because the merge queue then runs every test on the commit that lands; every other event, a pull request into `release/**` included, runs every test.

#### Scenario: Check commands
- **WHEN** the checks run
- **THEN** `fmt + clippy` runs `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
- **AND** `test` builds every test target and runs the tests with `cargo nextest run --workspace --locked` plus a `gitpm --version`/`--help` smoke test, in the same hermetic Git environment as `cargo test` (`.cargo/config.toml`)
- **AND** `msrv (core crate)` runs `cargo check -p gitpm -p gitpm-cli --all-targets --locked` with Rust 1.92

#### Scenario: Fast tier on a pull request
- **WHEN** a pull request into `main` is opened or updated
- **THEN** every required check reports on it
- **AND** `test` runs the nextest profile `pr`, which leaves out the long scenario tests named in `.config/nextest.toml`

#### Scenario: Full tier in the merge queue
- **WHEN** a merge queue group or a push to `release/**` is checked
- **THEN** `test` runs every test of the workspace: the default nextest profile without a filter, and the doctests

#### Scenario: Merge into main
- **WHEN** the merge queue merges a pull request into `main`
- **THEN** the push to `main` does not start CI

#### Scenario: MSRV break
- **WHEN** a change uses an API newer than Rust 1.92 in `gitpm` or `gitpm-cli`
- **THEN** the `msrv (core crate)` check fails and the PR cannot merge

#### Scenario: Release PR is checked
- **WHEN** a pull request into `release/0.6` is opened or updated
- **THEN** the same checks run on it as on a pull request into `main`
- **AND** `test` runs the full tier, as release branches have no merge queue

## ADDED Requirements

### Requirement: Nightly CI run
CI SHALL run every check with the full test tier on `main` once a night and on a manual run (`workflow_dispatch`). Build caches SHALL be saved only where pull requests can restore them: the merge queue SHALL NOT save them, and the nightly run on `main` keeps them warm.

#### Scenario: Nightly run
- **WHEN** the nightly schedule fires
- **THEN** CI checks the head of `main` with every test, as in the merge queue

#### Scenario: Caches for pull requests
- **WHEN** a merge queue group is checked
- **THEN** it restores build caches but does not save them
- **AND** a pull request restores the caches the nightly run saved on `main`
