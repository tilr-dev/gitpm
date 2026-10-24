## ADDED Requirements

### Requirement: Workspaces are isolated
Every public workspace operation SHALL accept an explicit workspace root, either directly or through a Manager or FFI handle opened with that root. Every CLI workspace command SHALL accept `--root`; the documented default MAY remain. Workspace state, clones, deployed files, build storage, transactions and locks SHALL be scoped to the selected root. Operations on one workspace SHALL NOT change another workspace's state or files, even when both contain the same package IDs or their roots have the same basename in different parent directories. Repository-only operations SHALL use their explicit repository paths without opening an implicit workspace.

#### Scenario: Preparing a second workspace
- **GIVEN** two workspaces in different directories containing the same package
- **WHEN** a package is installed or removed in one workspace
- **THEN** the other workspace's state and files keep their contents
- **AND** both workspaces can remain open through independent Manager handles

#### Scenario: Explicit command selection
- **GIVEN** the process working directory and the environment point at a different workspace
- **WHEN** an existing public CLI workspace command is invoked with an explicit `--root`
- **THEN** the command uses the selected root
- **AND** the workspace selected by the working directory or environment keeps its state and files

#### Scenario: Repository-only operation
- **WHEN** a repository-only command receives an explicit repository path
- **THEN** it operates on that path without opening a default workspace
