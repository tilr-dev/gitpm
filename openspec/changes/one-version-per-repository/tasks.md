## 1. Specification
- [x] 1.1 Describe shared repository versions, `vVERSION` discovery, authoring and fork behavior in the authorized requirement deltas.
- [x] 1.2 Align catalog, update, install, CLI, FFI and recorded-mode examples; remove library-owned ref transaction input.

## 2. Implementation
- [x] 2.1 Update release-tag creation and discovery, sharing versions across all repository packages.
- [x] 2.2 Update init and commit, preserving tag immutability and Git identity checks; keep fork limited to creating a repository.
- [x] 2.3 Update README and integration tests for repository releases, signed tags and retained upstream tags.

## 3. Validation
- [x] 3.1 Run focused integration coverage and formatting/lint checks.
- [x] 3.2 Complete self-review and required repository validation.

## 4. Release membership
- [x] 4.1 Ignore repository tags without the requested package ID, without warnings; keep version mismatches broken.
- [x] 4.2 Make origin checks verify package membership without changing installed clones or workspace state.
- [x] 4.3 Reconcile scenarios with repository tags and plain fork, and provide SSH signing in the Linux test image.
- [ ] 4.4 Verify regressions and review the final repository changes.
