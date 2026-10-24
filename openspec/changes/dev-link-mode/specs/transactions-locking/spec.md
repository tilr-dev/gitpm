## ADDED Requirements

### Requirement: Link transactions own entries and never their targets
Transactions SHALL stage and journal the deployed symlink entry, then publish state last, under the existing recovery protocol. Cleanup, removal and crash recovery MUST NOT follow a deployed link or recurse into its target, including dangling or externally retargeted links. The leaf symlink exception MUST NOT relax "Managed paths are never followed through symlinks" for managed parent directories. Atomicity SHALL cover workspace entries and state, not externally edited content.

#### Scenario: Failure before commit
- **WHEN** a link operation fails before the committed journal is written
- **THEN** staged symlink entries are discarded, and the previous workspace and external folder remain unchanged

#### Scenario: Interrupted link commit
- **WHEN** a process stops after the journal commits and before all symlink-entry and state renames finish
- **THEN** recovery completes the recorded entry changes and state idempotently without traversing or modifying the author's folder

#### Scenario: External edits during recovery
- **WHEN** the author edits or removes the target after journaling and before recovery
- **THEN** recovery operates on workspace symlink entries only and does not restore, delete or overwrite external content

#### Scenario: Managed parent is a symlink
- **WHEN** `dist/<namespace>` is a symlink outside the workspace
- **THEN** link deployment or removal refuses with `workspace` without writing or deleting through that parent
