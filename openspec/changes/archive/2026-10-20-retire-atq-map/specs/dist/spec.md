## ADDED Requirements

### Requirement: Repeating enable and disable is idempotent
When dependency and replacement guards permit it, repeating enable or disable SHALL succeed with CLI exit 0, preserving workspace state and clone bytes. Already reconciled deployed files SHALL remain byte-identical; otherwise the operation SHALL reconcile dist as required by "Every mutating command reconciles all of dist".

#### Scenario: Repeat enable
- **WHEN** an enabled package is enabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical

#### Scenario: Repeat disable
- **WHEN** a disabled package is disabled again with no intervening workspace changes
- **THEN** the operation succeeds and workspace state, clone contents and deployed files remain byte-identical

#### Scenario: Repeat enable repairs missing dist
- **WHEN** an enabled package is enabled again while another active package has a missing deployed folder
- **THEN** the operation reconciles dist and restores that folder, preserving workspace state and clone bytes
