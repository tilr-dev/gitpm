## 1. Research deliverables

- [x] 1.1 Compare origin following, live linking and combined semantics; verify against single-ref-mode and the owner decisions of 2026-10-19 in design.md.
- [x] 1.2 Describe link state, FFI, shared publishing-folder selection, transactions, recovery and lost-work boundaries; verify each in design.md and the deltas.
- [x] 1.3 Validate this change with `openspec validate dev-link-mode --strict --no-interactive` and review each delta against current specs.
- [x] 1.4 Align fork scenarios with repository-only preparation followed by explicit link installation; document provenance, examples and future verification without automatic release or replacement.

- [x] 1.5 Specify validated top-level replacement offers for link records and explicit replacement with a linked fork through the existing API, without creating bindings during link.

## 2. Implementation

- [x] 2.1 Reuse the manifest `dist` field with the existing shared `"dist"` default and explicit `"."` for the package root; implement explicit library/FFI link operation, root and multi-package selection, validation and CLI rendering; verify hermetic link integration tests and update README command examples.
- [x] 2.2 Implement discriminated link records under the existing state format policy; verify serialization, foreign-format refusal and readState/list parity tests; update README format/recreation guidance (BREAKING, no migration).
- [x] 2.3 Implement transactional link entry deployment/removal/recovery and lifecycle handling; verify failpoint integration tests plus external dirty/stashed/unpublished work preservation and managed-parent symlink refusal; document ownership/atomicity in README.
- [x] 2.4 Implement fixed link candidates and explicit availability/discovery results in check/update/update-all/dry-run through library/FFI/CLI; verify local-origin new-release, unreachable-origin and dependency integration tests; document reporting in README.
- [ ] 2.5 Verify `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and OpenSpec validation through the implementation task's prescribed full check runner before marking implementation complete.
- [x] 2.6 Verify the fork-to-link workflow with hermetic integration tests: fork preserves workspace state/dist/tags; explicit link installs only the selected package without a new release or replacement; library/FFI discovery uses validated nested fork provenance until relink refreshes it. Verify validated top-level `replaces`, explicit replacement with a linked fork through the existing API, live consumers and replacement/dependency checks; external offer edits remain inactive until validated relink. Document the separate preparation, live-install and explicit replace commands.
