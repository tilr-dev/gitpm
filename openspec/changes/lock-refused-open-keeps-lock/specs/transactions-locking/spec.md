## MODIFIED Requirements

### Requirement: Locks are process-owned and survive crashes
The workspace lock and the catalog lock SHALL be POSIX record locks (or open-file-description/`flock` locks) owned by the process, not inherited by child processes (git subprocesses never keep them alive), released when the owner dies, and tracked in a process-wide set keyed by the canonical path. A second lock attempt in the same process SHALL be refused from that set before the lock file is opened, so a refused attempt never opens and closes a second descriptor of a held file and never releases the first holder's lock. The lock files themselves SHALL be kept. Verified by `lock::lock_is_exclusive_across_processes_and_released_by_a_dead_owner`, `lock::released_lock_is_free_while_other_threads_spawn_subprocesses` and `lock::refused_second_open_keeps_the_first_lock`.

#### Scenario: Killed owner
- **WHEN** the process holding the lock is killed
- **THEN** the next open succeeds immediately

#### Scenario: Refused second Manager in the same process
- **WHEN** a host holds a Manager on a workspace, opens a second Manager on the same workspace in the same process (also through a symlinked spelling), gets `locked`, and another process then opens the workspace
- **THEN** the other process still fails with `locked`, because the refused attempt did not release the first Manager's lock

#### Scenario: Refused second catalog writer in the same process
- **WHEN** a catalog write holds `.<catalog name>.catalog.lock` and the same process starts a second write to that catalog, which fails with `locked`
- **THEN** a writer in another process still fails with `locked` until the first write ends
