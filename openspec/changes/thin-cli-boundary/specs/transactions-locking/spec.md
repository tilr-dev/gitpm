## MODIFIED Requirements

### Requirement: Workspace lock
Single source for which commands open a workspace: a Manager SHALL hold an exclusive lock on `.<workspace root name>.lock` beside the workspace root for its lifetime, keyed by the real (canonical) path so a symlinked spelling takes the same lock. A second Manager on the same workspace SHALL fail with `locked` ("workspace is already in use"). Read-only views SHALL NOT open a Manager. Verified by `recovery_and_lock` and `workspace::symlinked_workspace_root_shares_the_lock`.

#### Scenario: Who opens a workspace
- **WHEN** a CLI workspace command runs or an FFI `PluginManager` method is called
- **THEN** it opens a Manager: it takes the lock, creates a missing workspace root (workspace-layout) and runs crash recovery; the CLI `list` and `check` and the FFI free functions `list`, `check`, `readState` and `readInstalled` read `state.json` without opening, locking or creating anything

#### Scenario: Two CLI processes
- **WHEN** one `gitpm` process is installing and another runs `gitpm enable` on the same workspace root
- **THEN** the second fails with `locked`, while `gitpm list` in the meantime succeeds and reports the last committed `state.json`
