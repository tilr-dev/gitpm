## MODIFIED Requirements

### Requirement: Known manifest keys
The manifest SHALL require `id` (a valid package ID) and `version` (package-ids, "Version format"); it MAY carry `dependencies`, `forkOf` `{id, version, commit, source?}` and `replaces` `{id, version}`, which take no unknown keys. Their IDs MUST be valid and differ from `id`, their versions MUST pass the version format, and `forkOf.source` MUST NOT be empty. Verified by `model::manifest_parses_minimal_and_fork_manifests` and `model::manifest_rejections_have_precise_kinds`.

#### Scenario: Minimal manifest
- **WHEN** `plugin.json` is `{"id": "acme/greeter", "version": "0.1.0"}`
- **THEN** it is a valid manifest with no dependencies

#### Scenario: Invalid forkOf
- **WHEN** `forkOf.id` equals the manifest's own `id`, or `forkOf.commit` is not a 40- or 64-character hex commit, or `forkOf.version` is empty or not MAJOR.MINOR.PATCH, or `forkOf.source` is an empty string
- **THEN** the manifest is refused with `invalid_manifest` (own ID, malformed commit, empty version or source), or with `invalid_version` when `forkOf.version` is not MAJOR.MINOR.PATCH (rule in package-ids, "Version format")

#### Scenario: Self-replacement
- **WHEN** `replaces.id` equals the manifest's own `id`
- **THEN** the manifest is refused with `invalid_manifest` ("a plugin cannot replace itself")

#### Scenario: Fork manifest without source
- **WHEN** a fork's manifest has `forkOf` without `source`
- **THEN** it is valid; `check` then knows the original only from the catalog
