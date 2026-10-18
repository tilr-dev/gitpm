## ADDED Requirements

### Requirement: Host compatibility field
The manifest MAY carry the known key `host` `{"<name>": "<range>"}`: each name is a non-empty string chosen by the host and each range uses the dependency range syntax (dependencies, "Requirement syntax"); anything else SHALL be refused with `invalid_manifest`. gitpm knows no host name and only compares the range with the caller's host (updates). The key SHALL be written back unchanged. Verified by `model::manifest_host_ranges_are_validated`.

#### Scenario: Valid host field
- **WHEN** `plugin.json` has `"host": {"tilr": ">=3.0.0"}`
- **THEN** the manifest is valid and the range is reported as is

#### Scenario: Invalid range
- **WHEN** `plugin.json` has `"host": {"tilr": "three"}` or `"host": {"": "^1.0.0"}`
- **THEN** reading it fails with `invalid_manifest`
