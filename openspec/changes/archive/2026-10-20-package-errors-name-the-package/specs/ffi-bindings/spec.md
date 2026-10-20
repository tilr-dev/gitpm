## MODIFIED Requirements

### Requirement: Errors keep kind and message
Library errors SHALL map to the `PmError` enum, one case per kind, each with the full `message` and the library's structured fields (`id` for every error about a package, `LocalChanges.items`, `HasDependents.dependents` and `lostWork`, `IncompatibleReplacement.consumers`, `NotDeployed.sync`); `Other { kind, message }` SHALL carry a kind added to the non-exhaustive library enum later. Verified by `every_library_error_maps_to_its_own_case_with_the_full_message` and `ids_and_lost_work_items_are_preserved`.

#### Scenario: Local changes from Swift
- **WHEN** an unconfirmed uninstall would lose work
- **THEN** Swift receives `PmError.LocalChanges` with the message and the list of items

#### Scenario: Undeployed packages from Swift
- **WHEN** `syncDist` leaves an active package undeployed
- **THEN** Swift receives `PmError.NotDeployed` with the message and the `sync` report (what was deployed and removed, and the skipped packages with their reasons), as the CLI prints it

#### Scenario: Update refusal from Swift
- **WHEN** a branch-mode `update` of `acme/greeter` is refused because the history diverged
- **THEN** Swift receives `PmError.VersionConflict` whose `id` is `acme/greeter` and whose message names it

#### Scenario: Manifest refusal from Swift
- **WHEN** an update of `acme/greeter` finds another package ID or rollback finds a mismatched manifest
- **THEN** Swift receives `PmError.InvalidManifest` with `id` `acme/greeter` and the complete library message

#### Scenario: Unscoped validation error from Swift
- **WHEN** manifest or version validation fails without a package operation
- **THEN** the error keeps its original message and its optional `id` is absent
