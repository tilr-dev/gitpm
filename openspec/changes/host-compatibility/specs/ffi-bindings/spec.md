## ADDED Requirements

### Requirement: Host in ManagerOptions
`ManagerOptions` SHALL carry an optional `host` `{name, version}` passed to the library unchanged, and `PmError.IncompatibleHost` SHALL carry the package, host and range; records SHALL report each package's `host` map and the check report's `needsHost`. Verified by `records_convert_field_by_field`.

#### Scenario: Host from Swift
- **WHEN** a host opens a `PackageManager` with `ManagerOptions(host: HostInfo(name: "tilr", version: "2.5.0"))` and installs a package that needs `tilr >=3.0.0`
- **THEN** it receives `PmError.IncompatibleHost` and nothing is installed
