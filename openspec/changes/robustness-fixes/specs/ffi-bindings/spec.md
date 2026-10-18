## ADDED Requirements

### Requirement: SSH setting in ManagerOptions
`ManagerOptions` SHALL carry `customSsh` (the library's `Options.custom_ssh`), used for every git command of the opened workspace, and `authCheck(source, customSsh)` SHALL take the same setting, so a host decides once whether its environment brings its own SSH command. Verified by `records_convert_field_by_field`.

#### Scenario: Host with its own SSH command
- **WHEN** a host that sets `GIT_SSH` for its process opens a `PluginManager` with `customSsh: true` and installs from an SSH URL
- **THEN** gitpm passes no SSH command of its own and git uses `GIT_SSH`, as `authCheck` does
