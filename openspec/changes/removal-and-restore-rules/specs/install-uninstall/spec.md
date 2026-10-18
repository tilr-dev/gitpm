## ADDED Requirements

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a package recorded in state whose clone no longer exists SHALL succeed without confirmation (there is no lost work): they drop the record and its deployed folder, keep no `forgotten` record, and are still guarded by dependents (dependencies). Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `packages/acme/greeter` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** the record is dropped and no `forgotten` record is kept, because there is nothing to restore

## MODIFIED Requirements

### Requirement: Uninstall is planned and confirmed
`uninstall_plan(id)` SHALL be read-only and list `deletes`, `registered`, `dependents` (dependencies) and `lost_work`. Single source for lost-work items: `Modified`, `Untracked`, `Ignored`, `HeadMoved`, `UnpublishedCommit`, `UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall(id, confirmed, force)` SHALL refuse with `local_changes` when the clone has lost work not in `confirmed`, and with `has_dependents` unless `force`. A clean clone needs no confirmation.

#### Scenario: Dependents without force
- **WHEN** the package has dependents and `force` is false
- **THEN** `uninstall` fails with `has_dependents` (dependencies), and the error carries both `dependents` and the plan's `lost_work`, so one refusal tells everything

#### Scenario: Stash alone blocks
- **WHEN** the clone is clean except for a `git stash` entry and uninstall is not confirmed
- **THEN** it fails with `local_changes` listing the stash and nothing is deleted
- **AND** this is covered by `uninstall::plan_lists_each_lost_work_type_including_stash_without_changes`

#### Scenario: Confirmed uninstall
- **WHEN** the plan has lost work and the caller passes exactly those items as `confirmed`
- **THEN** the clone, `dist/<id>` and the state entry are deleted

#### Scenario: Lost work grew after the plan
- **WHEN** a file is added to the clone after the plan was shown and the caller confirms only the plan's items
- **THEN** `uninstall` fails with `local_changes` listing the new item and nothing is deleted
- **AND** this is covered by `uninstall::confirmation_is_bound_to_the_planned_lost_work`

### Requirement: CLI confirmation flow for uninstall
Without `--force` the CLI SHALL list what `uninstall_plan` reports and ask `Delete anyway? [y/N]` on stderr on a TTY; anything but `y`/`yes` (EOF too) SHALL change nothing and fail with `uninstall cancelled; nothing was changed`. Without a TTY it calls the library unconfirmed, which refuses. `--force` confirms the plan's items without asking and ignores dependents. Verified by `uninstall_on_tty_asks_before_losing_work` and `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and the exit status is 1
- **AND** a declined prompt is a CLI error without `Error kind` (exit status 1), and `--force` passes the plan's items as `confirmed` and sets `force`

