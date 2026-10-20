# Tasks

## 1. Specification

- [x] 1.1 Reconcile refusals after TILR-39: same-version commits remain allowed; scope errors, FFI and updates deltas to remaining refusals.

## 2. Library and bindings

- [x] 2.1 Package-scoped update/rollback invalid_manifest and version_conflict errors carry structured ID and complete messages; global errors retain their message without a package ID.
- [x] 2.2 Local-work refusals of update/rollback/update all name the package and list items; CLI prints the library message unchanged; FFI preserves ID and items.

## 3. Verification

- [x] 3.1 Reproduce missing ID with deterministic regression tests, including package_errors_name_the_package and ids_and_lost_work_items_are_preserved.
- [x] 3.2 Run task bin/check and finish workflow.
