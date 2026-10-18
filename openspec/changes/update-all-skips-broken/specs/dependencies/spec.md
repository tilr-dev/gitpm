## MODIFIED Requirements

### Requirement: Broken consumers are reported, not enforced
After a forced removal, already broken consumers SHALL be reported (in `list` dependency entries, `check` "Unsatisfied dependency" lines and as skipped in `update all`) but SHALL NOT block unrelated operations; `update all` SHALL leave them out of its solve and SHALL never reinstall a dependency that was removed with force. Verified by `update_all_skips_broken_consumers_and_never_reinstalls`.

#### Scenario: Unrelated install after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, and the user installs `other`
- **THEN** the install succeeds and `list` reports `app`'s dependency on `lib` as unsatisfied

#### Scenario: update all after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, a catalog that has `lib` is configured, and the user runs `update all`
- **THEN** the other packages are updated, `app` is kept back with the reason `dependency acme/lib is not installed`, and `lib` is not installed again
