## 1. Specification and cause
- [x] 1.1 Specify source-owned compilation artifacts.
- [x] 1.2 Reproduce stale artifacts with controlled source timestamps and record evidence.

## 2. Implementation
- [x] 2.1 Scope target caches to the build source snapshot for builds and clippy.
- [x] 2.2 Add a deterministic regression using the production build path and parallel workspaces.

## 3. Validation
- [x] 3.1 Show regression fails without the fix and passes with it; verify cache reuse.
- [x] 3.2 Run required checks and self-review, then prepare for Review.
