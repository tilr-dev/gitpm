## ADDED Requirements

### Requirement: commit returns what it released
`commit_package` SHALL return `{package, version, tag}` of the release it made (the tag as in package-ids, "Release tag names"), and the CLI SHALL print its line from that result without computing the tag itself. Verified by `scaffold::commit_package_returns_package_version_and_tag`.

#### Scenario: Result of a release
- **WHEN** the author runs `gitpm commit ./tilr-plugins 0.2.0 --package tilr/store`
- **THEN** the library returns package `tilr/store`, version `0.2.0` and tag `tilr/store@0.2.0`, and the CLI prints `tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)`
