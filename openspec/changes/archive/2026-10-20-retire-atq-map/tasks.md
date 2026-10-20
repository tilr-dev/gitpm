## 1. Reconciliation and specifications

- [x] 1.1 Reconcile each of the 314 historical checks against current requirements/scenarios; record exact references and supersessions in design.md.
- [x] 1.2 Add any missing observable requirements/scenarios in the matching capability deltas; verify against existing tests and current implementation.
- [x] 1.3 Remove the historical map and verify no references remain in tracked repository files.

## 2. Validation

- [x] 2.1 Run `openspec validate --all --strict` and verify all reconciliation references resolve.
- [x] 2.2 Run the required checks through `bin/check` (including `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`); archive through `bin/finish`.
