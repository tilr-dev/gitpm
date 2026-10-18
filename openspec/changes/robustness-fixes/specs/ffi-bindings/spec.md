## ADDED Requirements

### Requirement: SSH setting in ManagerOptions
`ManagerOptions` SHALL carry `customSsh` (the library's `Options.custom_ssh`), used for every git command of the opened workspace, and `authCheck(source, customSsh)` SHALL take the same setting, so a host decides once whether its environment brings its own SSH command. Verified by `records_convert_field_by_field`.

#### Scenario: Host with its own SSH command
- **WHEN** a host that sets `GIT_SSH` for its process opens a `PackageManager` with `customSsh: true` and installs from an SSH URL
- **THEN** gitpm passes no SSH command of its own and git uses `GIT_SSH`, as `authCheck` does

## MODIFIED Requirements

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id` for every error about a package, `LocalChanges.items`, `HasDependents.dependents` and `lostWork`, `IncompatibleReplacement.consumers`, `NotDeployed.skipped`, `IncompatibleHost` package, host and range); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later. Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items

#### Scenario: Undeployed packages from Swift
- **WHEN** `syncDist` leaves an active package undeployed
- **THEN** Swift receives `PmError.NotDeployed` with the message and the skipped packages with their reasons

#### Scenario: Update refusal from Swift
- **WHEN** a branch-mode `update` of `acme/greeter` is refused because the history diverged
- **THEN** Swift receives `PmError.VersionConflict` whose `id` is `acme/greeter` and whose message names it
