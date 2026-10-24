## ADDED Requirements

### Requirement: Workspaces are isolated
Every public workspace operation SHALL take an explicit root, directly or through a Manager or FFI handle. CLI workspace commands SHALL accept `--root`; the documented default MAY remain. State, clones, deployed files, builds, transactions and locks SHALL belong to the selected root. Operations SHALL NOT change another workspace's state or files. Repository-only operations SHALL use explicit repository paths without opening an implicit workspace.

#### Scenario: Preparing a second workspace
- **GIVEN** two workspaces with the same root basename in different parent directories containing the same package IDs
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
