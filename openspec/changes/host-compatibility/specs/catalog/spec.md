## ADDED Requirements

### Requirement: Host ranges in the catalog
A release record SHALL carry the `host` map as part of its manifest (catalog, "Release record"); `install-catalog` SHALL choose the newest catalog version that fits the caller's host (updates, "Releases are chosen for the host"), and `check` SHALL show for a newer version that does not fit which host it needs instead of offering it. Verified by `catalog::check_names_the_host_a_newer_release_needs`.

#### Scenario: check with a newer release for another host version
- **WHEN** the host is `tilr@2.5.0`, `acme/greeter` 1.4.0 is installed and the catalog's latest 2.0.0 needs `tilr >=3.0.0`
- **THEN** the row says `up to date` for this host and notes `2.0.0 needs tilr >=3.0.0`; the report carries it as `needs_host`
