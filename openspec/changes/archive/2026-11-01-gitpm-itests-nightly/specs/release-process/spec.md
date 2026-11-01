## MODIFIED Requirements

### Requirement: Nightly CI run
CI SHALL run every check with the full test tier on `main` once a night and on a manual run (`workflow_dispatch`). Build caches SHALL be saved only where pull requests can restore them: the merge queue SHALL NOT save them, and the nightly run on `main` keeps them warm. A nonrequired Ubuntu `itests` job SHALL additionally run the complete Dagger suite on nightly `main`, manual runs and pushes to `release/**` only, with bounded compilation and test concurrency.

#### Scenario: Nightly run
- **WHEN** the nightly schedule fires
- **THEN** CI checks the head of `main` with every test from the full merge-queue tier and the complete Dagger suite

#### Scenario: Caches for pull requests
- **WHEN** a merge queue group is checked
- **THEN** it restores build caches but does not save them
- **AND** a pull request restores the caches the nightly run saved on `main`

#### Scenario: Dagger job routing
- **WHEN** CI runs on nightly main, workflow_dispatch or a push to release/**
- **THEN** it runs the complete Dagger suite with bounded concurrency on the local engine
- **AND** the CLI matches the module engine version and its download is verified with a pinned SHA-256
- **AND** the itests job is absent from required checks and does not run on pull_request or merge_group

#### Scenario: Reuse the nightly engine cache
- **WHEN** a subsequent eligible run finds a cache saved by a successful nightly run on main
- **THEN** it restores cargo home and source-owned target cache volumes before starting Dagger
- **AND** only successful nightly main runs save the engine cache, after stopping the engine; manual runs and release pushes do not save caches

#### Scenario: Failed Dagger tests retain reports
- **WHEN** a suite run exports results containing failed cases
- **THEN** the job fails and uploads results/ as an artifact for investigation
