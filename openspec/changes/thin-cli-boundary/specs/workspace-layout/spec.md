## ADDED Requirements

### Requirement: Options are validated before anything is created
`Manager::open_with` SHALL validate its root path and Options (for example the catalog location, catalog "A configured catalog location must exist") before it creates the workspace root, its folders, `state.json` or the lock file; a refusal SHALL leave the file system unchanged. Verified by `workspace::open_with_validates_options_before_creating_anything`.

#### Scenario: Invalid options on a new root
- **WHEN** a host opens `new-ws`, which does not exist, with an invalid Options value
- **THEN** opening fails, and neither `new-ws` nor `.new-ws.lock` exists afterwards
