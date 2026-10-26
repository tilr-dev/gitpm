## MODIFIED Requirements

### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`repos`, `worktrees`, `dist`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed` and `a_uuid_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Symlinked namespace
- **WHEN** `repos` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `6d082077-2279-5cc6-b47f-b78cd35cec24` refuse and nothing outside the workspace root changes

#### Scenario: Blocked dist namespace
- **WHEN** `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced` is a file or symlink and `d135a7cf-1c63-549a-93cb-8f89656b2ced` is active
- **THEN** an operation that deploys or removes `d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `workspace` and does not write or delete through `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced`, while a command on an unrelated package can proceed

#### Scenario: Symlinked worktree parent
- **WHEN** `worktrees` or the affected `worktrees/<uuid>` entry is a symlink or a non-directory
- **THEN** an operation using that managed checkout path refuses with `workspace` and changes nothing through it

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
- **WHEN** `dist` is a symlink outside the workspace
- **THEN** link deployment or removal refuses with `workspace` without writing or deleting through that parent
