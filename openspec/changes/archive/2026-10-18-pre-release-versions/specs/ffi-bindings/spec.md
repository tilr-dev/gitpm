## ADDED Requirements

### Requirement: Major and pre-release flags in records
FFI records SHALL carry the version flags of the library results: `RevisionChange.major`, `major` on each `UpdateAllPlan` update entry, and `major` and `prerelease` on each `CheckReport` row. This adds to "Records mirror library results" and "Check report record". Verified by `records_convert_field_by_field`.

#### Scenario: Major update from Swift
- **WHEN** a host updates `acme/greeter` from 1.2.3 to 2.0.0
- **THEN** the returned `RevisionChange` has `major` true, as the CLI line ends with ` (major)`

#### Scenario: Pre-release in a check row
- **WHEN** 1.2.0 is installed and the catalog has 2.0.0 and 2.1.0-rc.1
- **THEN** the host's `CheckReport` row has `major` true and `prerelease` `2.1.0-rc.1`
