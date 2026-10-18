## ADDED Requirements

### Requirement: Kept-back reasons in records
Each `keptBack` entry of the FFI `UpdateAllPlan` SHALL carry the library's `HeldBack.reason`, so a host explains a kept-back package as `gitpm update all` does. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Broken consumer from Swift
- **WHEN** `lib` was force-uninstalled while `app` depends on it and a host calls `updateAllPlan`
- **THEN** the plan's `keptBack` has `app` with `reason` `dependency acme/lib is not installed`
