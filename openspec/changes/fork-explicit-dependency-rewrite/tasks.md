## 1. Explicit rewrite
- [x] 1.1 Update the authorized requirement deltas and scenarios.
- [x] 1.2 Add library rewrite API and preserve dependencies in installed/source forks; verify both paths and idempotence.
- [x] 1.3 Add FFI and CLI entry points, parity coverage and next-step output tests.
- [x] 1.4 Update README breaking-change documentation and affected tests.

## 2. Validation
- [x] 2.1 Run focused integration tests and formatting/clippy checks.
- [ ] 2.2 Review the diff with a subagent and resolve findings.
- [ ] 2.3 Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings and cargo test --workspace through the workflow finish gate.
