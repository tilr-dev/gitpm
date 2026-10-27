## 1. Contract and library
- [x] 1.1 Add authorized spec deltas and new inspection/edit scenarios; verify OpenSpec validation.
- [x] 1.2 Implement repository inspection and extend installed listing; verify library integration tests for root/nested/original/fork packages.
- [x] 1.3 Implement explicit dependency edits, validation and metadata preservation; replace inferred rewrite tests with add/update/remove and fork-to-edit integration coverage.

## 2. Bindings and CLI
- [x] 2.1 Expose inspection and edits in FFI with field-by-field records and integration tests.
- [x] 2.2 Add CLI inspection/edit commands, fork next steps and parity entries; verify CLI integration tests including invalid edits without writes.
- [x] 2.3 Remove old rewrite surfaces and update README with breaking-change migration; verify repository search.

## 3. Validation
- [x] 3.1 Run focused integration tests and formatting/clippy checks.
- [x] 3.2 Review the complete diff with a subagent and resolve findings.
- [x] 3.3 Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings and cargo test --workspace through the finish gate.
