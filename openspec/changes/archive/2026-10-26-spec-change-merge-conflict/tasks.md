## 1. Specification
- [x] Add the merge readiness diagnostics requirement and scenarios.

## 2. Implementation
- [x] Add trusted merge-fetch logic with API conflict detection and bounded waiting.
- [x] Document trusted deployment and required permissions.

## 3. Validation
- [x] Cover pending, conflict, stale API heads and command failures with deterministic fixtures in the shared helper.
- [x] Verify shared helper deployment without component-local copies.
