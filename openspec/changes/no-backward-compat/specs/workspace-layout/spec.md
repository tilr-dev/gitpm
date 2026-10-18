## MODIFIED Requirements

### Requirement: State format versions
`state.json` SHALL carry `"format": 2`. A file with any other format, or without one, SHALL be refused with `unsupported` and MUST NOT be rewritten; there is no migration. The format SHALL be bumped with every change that an older gitpm would misread. Verified by `model::state_format_other_than_current_is_refused`.

#### Scenario: Workspace from gitpm 0.3
- **WHEN** a Manager opens a workspace whose `state.json` has no `format`
- **THEN** opening fails with `unsupported`, the message names the found and the supported format, and `state.json` keeps its bytes

#### Scenario: Newer format
- **WHEN** `state.json` has `"format": 3`
- **THEN** opening the state fails with `unsupported`

### Requirement: Downgrade is not protected
A workspace SHALL be usable only by a gitpm that writes its state format, in either direction. README SHALL list the state format of each release and say that a workspace of another format is re-created (its packages installed again), never migrated.

#### Scenario: Documented downgrade boundary
- **WHEN** a user reads README "Rules of the library"
- **THEN** it lists the state format per gitpm release and says that a workspace of another format is refused with `unsupported`
