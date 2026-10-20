## ADDED Requirements

### Requirement: Build artifacts belong to their source snapshot
The runner SHALL isolate mutable Cargo targets by complete build source snapshot and purpose, executing each worktree's own CLI, API example and Rust tests. Identical snapshots SHALL reuse compilation caches, Cargo downloads SHALL remain shared, and specification-only edits SHALL NOT invalidate compilation. The Rust group SHALL include a controlled ownership and reuse check as `rust:build-cache`, also available as `./itests/dt call cache-regression`.

#### Scenario: Different worktrees share an engine
- **WHEN** two worktrees with distinguishable executable behavior build on the same Dagger engine, sequentially or concurrently, and their source timestamps predate existing target outputs
- **THEN** each run executes the CLI, API example and Rust test executables corresponding to its own sources

#### Scenario: Unchanged build sources reuse compilation
- **WHEN** a run uses the same build source snapshot and build purpose as an earlier run, including after a specification-only edit
- **THEN** the same compilation cache is available without mixing artifacts with another source snapshot

#### Scenario: Standalone cache regression
- **WHEN** the developer runs `./itests/dt call cache-regression`
- **THEN** controlled sequential and concurrent snapshots and warm cache reuse are checked, and the command fails if ownership or reuse is broken
