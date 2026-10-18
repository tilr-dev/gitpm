## MODIFIED Requirements

### Requirement: Known manifest keys
The manifest SHALL require `id` (a valid package ID) and `version` (format in package-ids, "Version format") and MAY carry `dependencies` (see dependencies), `forkOf` `{id, version, commit}` and `replaces` `{id, version}` without unknown keys; their IDs MUST be valid and other than the own `id`, their versions MUST pass the version format. Verified by `model::manifest_parses_minimal_and_fork_manifests` and `model::manifest_rejections_have_precise_kinds`.

#### Scenario: Minimal manifest
- **WHEN** `plugin.json` is `{"id": "acme/greeter", "version": "0.1.0"}`
- **THEN** it is a valid manifest with no dependencies

#### Scenario: Invalid forkOf
- **WHEN** `forkOf.id` equals the manifest's own `id`, or `forkOf.commit` is not a 40- or 64-character hex commit, or `forkOf.version` is empty or not MAJOR.MINOR.PATCH
- **THEN** the manifest is refused with `invalid_manifest` (own ID, malformed commit, empty version), or with `invalid_version` when `forkOf.version` is not MAJOR.MINOR.PATCH (rule in package-ids, "Version format")

#### Scenario: Self-replacement
- **WHEN** `replaces.id` equals the manifest's own `id`
- **THEN** the manifest is refused with `invalid_manifest` ("a package cannot replace itself")
