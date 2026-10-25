## ADDED Requirements

### Requirement: Moved and copied workspaces keep usable worktrees
On open, a Manager SHALL repair stale Git links between existing managed checkouts and admin directories under the workspace lock, after journal recovery and state validation. This maintenance SHALL derive pairs from installed and forgotten state at the selected root and preserve checkout contents, HEAD, state and the source workspace.

#### Scenario: Copied workspace
- **GIVEN** an installed Git package and a copy of its complete workspace at another root
- **WHEN** the copy is opened and its packages are listed, updated, rolled back and uninstalled
- **THEN** those operations succeed, each retained worktree has a clean Git status, and the source workspace remains unchanged
- **AND** repair never follows stale links into the source workspace

#### Scenario: Moved workspace
- **GIVEN** a workspace moved to another path and its old path no longer exists
- **WHEN** a Manager opens it
- **THEN** installed and forgotten Git worktrees resolve to the new root without manual rebinding

#### Scenario: Repeated open
- **WHEN** a workspace with correct worktree links is opened again
- **THEN** its Git metadata is not rewritten
- **AND** this holds when the selected root contains a symlink alias

#### Scenario: Missing link files
- **GIVEN** a managed checkout and its admin directory both exist but `.git`, `gitdir` or `commondir` is missing
- **WHEN** a Manager opens the workspace
- **THEN** the missing link files are restored and the worktree remains usable
- **AND** other I/O errors are propagated

#### Scenario: Records without a worktree
- **WHEN** the workspace contains build or external link records, absent checkouts or absent registrations
- **THEN** opening does not recreate checkouts or registrations or require Git metadata for build and external link records

#### Scenario: Read-only view
- **WHEN** a read-only view reads a moved workspace without opening a Manager
- **THEN** it keeps its existing behavior without repairing links

#### Scenario: Unsafe metadata
- **GIVEN** a managed worktree metadata file or managed parent has been replaced by a symlink
- **WHEN** a Manager opens the workspace
- **THEN** opening refuses the unsafe path without writing through the symlink
