## 1. Research deliverables (TILR-35)

- [x] 1.1 Compare origin following, live linking and combined semantics; verify against single-ref-mode and Dan's comments #833–836 in design.md.
- [x] 1.2 Describe link state, FFI, installation-folder dependency, transactions, recovery and lost-work boundaries; verify each in design.md and the deltas.
- [x] 1.3 Validate this change with `openspec validate dev-link-mode --strict --no-interactive` and review each delta against current specs (passed 2026-10-20).

## 2. Future implementation (not completed by the research task)

- [ ] 2.1 Reuse TILR-142's installation-folder contract; implement explicit library/FFI link operation, root and multi-package selection, validation and CLI rendering; verify hermetic link integration tests and update README command examples.
- [ ] 2.2 Implement discriminated link records and a state format bump; verify serialization, foreign-format refusal and readState/list parity tests; update README format/recreation guidance (BREAKING, no migration).
- [ ] 2.3 Implement transactional link entry deployment/removal/recovery and lifecycle handling; verify failpoint integration tests plus external dirty/stashed/unpublished work preservation and managed-parent symlink refusal; document ownership/atomicity in README.
- [ ] 2.4 Implement fixed link candidates and explicit availability/discovery results in check/update/update-all/dry-run through library/FFI/CLI; verify local-origin new-release, unreachable-origin and dependency integration tests; document reporting in README.
- [ ] 2.5 Verify `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and OpenSpec validation through the implementation task's prescribed full check runner before marking implementation complete.
