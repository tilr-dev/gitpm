## ADDED Requirements

### Requirement: Install respects the host
`install`, `install-catalog` and `download` SHALL choose and verify the release for the caller's host as specified in updates ("Releases are chosen for the host"), before any workspace change; a missing dependency pulled from the catalog SHALL be chosen the same way. Verified by `workspace::releases_are_chosen_for_the_host`.

#### Scenario: Explicit ref for a newer host
- **WHEN** the host is `tilr@2.5.0` and the user runs `install SOURCE --ref acme/greeter@2.0.0`, whose manifest needs `tilr >=3.0.0`
- **THEN** it fails with `incompatible_host` and nothing is created
