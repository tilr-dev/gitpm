## ADDED Requirements

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a package recorded in state whose clone (own or shared) no longer exists SHALL succeed without confirmation (there is no lost work): they drop the record and its deployed folder, keep no `forgotten` record, and are still guarded by dependents (dependencies, "Dependents guard removal"). Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `packages/acme/greeter` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

## MODIFIED Requirements

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (keep everything, set `enabled: false`, remove `dist/<id>`), `forget` (drop the package from `packages` in state, keep the clone and a `forgotten` record, remove `dist/<id>`; no `forgotten` record when the clone is gone, "Removing a package whose clone is gone") and `uninstall` (remove the state entry, the clone and `dist/<id>`). `forget` returns `ForgetResult { clone_kept, dist }`, `clone_kept` false when the clone was gone. `restore ID` SHALL record a forgotten package in state again. All three removals are guarded as in dependencies ("Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels` and `forget_says_whether_the_clone_was_kept`.

#### Scenario: Uninstall one package of a shared clone
- **WHEN** another package still uses the shared clone of the package being uninstalled
- **THEN** the clone stays; it is removed only with its last package (multi-package-repos, "One shared clone per source")

#### Scenario: Forget then restore
- **WHEN** the user runs `forget acme/greeter` then `restore acme/greeter`
- **THEN** the first prints `acme/greeter forgotten (clone kept; restore re-registers it)`, the second `acme/greeter restored 0.2.0`, and `dist/acme/greeter/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID that has no `forgotten` record
- **THEN** the command fails with `repository_not_found`, as does a forgotten package whose clone (own or shared) is gone (multi-package-repos, "Forgotten packages of shared clones")

### Requirement: Uninstall is planned and confirmed
`uninstall_plan(id)` SHALL be read-only and list `deletes` (paths), `registered`, `dependents` (dependencies, "Dependents guard removal") and `lost_work`; `UninstallPlan::refusal` SHALL return the `has_dependents` error `uninstall` would raise without force (with the plan's `lost_work`), so a caller reports it without calling `uninstall`. Single source for the lost-work items: `Modified`, `Untracked`, `Ignored`, `HeadMoved`, `UnpublishedCommit`, `UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall(id, confirmed, force)` (FFI `uninstall(confirmed, force)`) SHALL refuse with `local_changes` (listing the items) when the clone has lost work not in `confirmed`, and with `has_dependents` unless `force`. A clean clone SHALL be removed without confirmation.

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
Without `--force`, the CLI SHALL report the plan's refusal when it has dependents, and otherwise list what `uninstall_plan` reports and ask `Delete anyway? [y/N]` on stderr on a TTY; anything but `y`/`yes` (EOF included) SHALL change nothing and fail with `uninstall cancelled; nothing was changed`, and `y` passes the plan's items as `confirmed`. Without a TTY it calls the library with nothing confirmed, which refuses. `--force` confirms the plan's items without asking and ignores dependents. Verified by `uninstall_on_tty_asks_before_losing_work` and `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and the exit status is 1
- **AND** a declined prompt is a CLI error without `Error kind` (exit status 1), and `--force` passes the plan's items as `confirmed` and sets `force`
