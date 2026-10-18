## ADDED Requirements

### Requirement: Releases are chosen for the host
With `Options.host` `{name, version}` set, install, `install-catalog`, `download`, update and `update all` SHALL pick the newest release whose manifest has no `host[name]` or one the host version satisfies (without it nothing is filtered). When none fits, or an explicit ref or version does not, they SHALL fail with `incompatible_host` naming package, host and range; `update all` keeps the package back (`needs NAME RANGE`). Verified by `workspace::releases_are_chosen_for_the_host`.

#### Scenario: Newest release needs a newer host
- **WHEN** the host is `tilr@2.5.0`, `acme/greeter` 1.4.0 has `"host": {"tilr": ">=2.0.0"}` and 2.0.0 has `"host": {"tilr": ">=3.0.0"}`
- **THEN** `update acme/greeter` moves to 1.4.0 at most, and `update all` lists it kept back with reason `needs tilr >=3.0.0`

#### Scenario: Nothing fits
- **WHEN** the host is `tilr@2.5.0` and every release of a package needs `tilr >=3.0.0`
- **THEN** `install` fails with `incompatible_host` ("acme/greeter needs tilr >=3.0.0; the host is tilr 2.5.0") and the workspace is unchanged

#### Scenario: Another host's entry
- **WHEN** the host is `tilr@2.5.0` and a release has only `"host": {"other": ">=9.0.0"}`
- **THEN** the release counts as compatible
