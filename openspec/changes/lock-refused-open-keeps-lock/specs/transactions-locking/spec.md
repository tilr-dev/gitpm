## MODIFIED Requirements

### Requirement: Locks are process-owned and survive crashes
Workspace and catalog locks SHALL be process-owned POSIX record locks, not inherited by children and released when the owner dies. A process-wide set keyed by canonical path SHALL refuse repeated attempts before opening the file, preserving the held lock. Lock files SHALL be kept.

#### Scenario: Killed owner
- **WHEN** the process holding the lock is killed
- **THEN** the next open succeeds immediately

#### Scenario: Refused second Manager in the same process
- **WHEN** a host holds a Manager on a workspace, opens a second Manager on the same workspace in the same process (also through a symlinked spelling), gets `locked`, and another process then opens the workspace
- **THEN** the other process still fails with `locked`, because the refused attempt did not release the first Manager's lock

#### Scenario: Refused second catalog writer in the same process
- **WHEN** a catalog write holds `.<catalog name>.catalog.lock` and the same process starts a second write to that catalog, which fails with `locked`
- **THEN** a writer in another process still fails with `locked` until the first write ends

#### Scenario: Lock regression coverage
- **WHEN** lock behavior is tested
- **THEN** `lock::lock_is_exclusive_across_processes_and_released_by_a_dead_owner`, `lock::released_lock_is_free_while_other_threads_spawn_subprocesses`, `lock::refused_second_open_keeps_the_first_lock`, and `catalog::tests::refused_second_catalog_writer_keeps_the_first_lock` verify process ownership, release, and refused attempts
