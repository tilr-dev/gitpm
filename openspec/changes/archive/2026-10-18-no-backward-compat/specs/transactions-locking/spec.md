## MODIFIED Requirements

### Requirement: Workspace lock
Single source for which commands open a workspace: a Manager SHALL hold an exclusive lock on `.<workspace root name>.lock` beside the workspace root for its lifetime, keyed by the real (canonical) path so a symlinked spelling takes the same lock. A second Manager on the same workspace SHALL fail with `locked` ("workspace is already in use"). Verified by `recovery_and_lock` and `workspace::symlinked_workspace_root_shares_the_lock`.

#### Scenario: Who opens a workspace
- **WHEN** a CLI workspace command runs (the read-only `list` included) or an FFI `PackageManager` method is called (`check` included)
- **THEN** it opens a Manager: it takes the lock, creates a missing workspace root (workspace-layout) and runs crash recovery; only the CLI `check` and the FFI free function `readInstalled` read `state.json` without opening the workspace

#### Scenario: Two CLI processes
- **WHEN** one `gitpm` process is installing and another runs `gitpm list` on the same workspace root
- **THEN** the second fails with `locked`

### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`packages`, `packages/<ns>`, `repos`, `dist`, `dist/<ns>`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed`.

#### Scenario: Symlinked namespace
- **WHEN** `packages/dan` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `dan/x` refuse and nothing outside the workspace root changes
