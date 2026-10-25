## MODIFIED Requirements

### Requirement: Known manifest keys
The manifest SHALL require id, name and version and MAY carry dist, buildCommand, buildRequired, dependencies, forkOf and replaces. IDs MUST be canonical UUIDs, name MUST be 1 to 80 ASCII letters, digits, `-` or `_` and MUST NOT start with `-` and versions MUST be valid; forkOf and replaces MUST name another ID and take no unknown keys. forkOf.source MUST be a Git URL or absolute path.

Verified by `model::manifest_parses_minimal_and_fork_manifests` and `model::manifest_rejections_have_precise_kinds`.

#### Scenario: Field shapes
- **WHEN** optional fields are supplied
- **THEN** dist is a nonempty package-relative string ("." selects the package root), buildCommand is a string, buildRequired is boolean, forkOf is {id, version, commit, source?}, and replaces is {id, version}
- **AND** forkOf and replaces versions follow package-ids "Version format"

#### Scenario: Minimal manifest
- **WHEN** `plugin.json` is `{"id": "d135a7cf-1c63-549a-93cb-8f89656b2ced", "name": "greeter", "version": "0.1.0"}`
- **THEN** it is a valid manifest with no dependencies

#### Scenario: Invalid forkOf
- **WHEN** `forkOf.id` equals the manifest's own `id`, or `forkOf.commit` is not a 40- or 64-character hex commit, or `forkOf.version` is empty or not MAJOR.MINOR.PATCH, or `forkOf.source` is empty, relative (`foo/bar`), a remote helper (`ext::sh -c id`, `fd::5`, any `::`) or a URL of another scheme than `https`, `http`, `ssh`, `git`, `file` (or `host:path`) or with credentials
- **THEN** the manifest is refused with `invalid_manifest` (own ID, malformed commit, empty version, invalid source), or with `invalid_version` when `forkOf.version` is not MAJOR.MINOR.PATCH (rule in package-ids, "Version format")

#### Scenario: Self-replacement
- **WHEN** `replaces.id` equals the manifest's own `id`
- **THEN** the manifest is refused with `invalid_manifest` ("a package cannot replace itself")

#### Scenario: Fork manifest without source
- **WHEN** a fork's manifest has `forkOf` without `source`
- **THEN** it is valid; `check` then knows the original only from the catalog

#### Scenario: Legacy manifest refused
- **WHEN** a manifest has a scoped ID or omits its name
- **THEN** it is refused without migrating or rewriting it

### Requirement: forkOf is provenance, replaces is an offer
`forkOf` SHALL only record where a fork came from; it MUST NOT redirect any consumer. `replaces` SHALL declare that the package can stand in for another ID at a version, and it SHALL take effect only after an explicit replace (see fork-replacement).

#### Scenario: Installing a fork changes no consumer
- **WHEN** a fork with `forkOf` and `replaces` for `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` is installed
- **THEN** consumers of `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` still resolve to `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` until `replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with FORK` is run
