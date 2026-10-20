## ADDED Requirements

### Requirement: Repeating enable and disable is idempotent
For an installed package whose dependencies and replacement guards permit the operation, enabling an already enabled package or disabling an already disabled package SHALL succeed without changing the bytes of workspace state, clones or deployed files. The CLI SHALL exit 0. Verified by `uc_020:repeated_disable_is_noop`, `uc_020:enable_of_enabled_plugin_is_noop`, `uc_062:enable_enabled` and `uc_062:disable_disabled`.

#### Scenario: Repeat enable
- **WHEN** an enabled package is enabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical

#### Scenario: Repeat disable
- **WHEN** a disabled package is disabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical
