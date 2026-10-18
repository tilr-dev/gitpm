## ADDED Requirements

### Requirement: Check report record
The free function `check(root, catalog)` SHALL return a `CheckReport` record that mirrors the library report field by field: rows (with `status`, message and `source` `catalog`/`origin`), the fork `upstream` of each row (with its `source`), the replacement bindings and the unsatisfied dependencies, so a host shows what `gitpm check` prints. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: check without a catalog from Swift
- **WHEN** a host calls `check(root, nil)` on a workspace with packages installed from Git
- **THEN** it receives a `CheckReport` whose rows have `source` `origin`, plus the replacement bindings and unsatisfied dependencies the CLI prints
