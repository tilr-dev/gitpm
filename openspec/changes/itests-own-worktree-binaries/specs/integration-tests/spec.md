## ADDED Requirements

### Requirement: Build artifacts belong to their source snapshot
The runner SHALL isolate mutable Cargo target artifacts by the complete build source snapshot and build purpose. Runs from different worktrees SHALL execute their own CLI, API example and Rust test executables even when they overlap or source file timestamps predate cached outputs. Identical build snapshots SHALL retain reusable compilation caches; Cargo downloads SHALL remain shared. Specification-only edits SHALL NOT invalidate the compilation cache.

`./itests/dt call cache-regression` SHALL run a controlled check of sequential and concurrent snapshots and warm compilation cache reuse, failing when artifact ownership or reuse is broken. The Rust group of the ordinary suite SHALL include this check as `rust:build-cache`.

#### Scenario: Different worktrees share an engine
- **WHEN** two worktrees with distinguishable executable behavior build on the same Dagger engine, sequentially or concurrently, and their source timestamps predate existing target outputs
- **THEN** each run executes the CLI, API example and Rust test executables corresponding to its own sources

#### Scenario: Unchanged build sources reuse compilation
- **WHEN** a run uses the same build source snapshot and build purpose as an earlier run, including after a specification-only edit
- **THEN** the same compilation cache is available without mixing artifacts with another source snapshot
