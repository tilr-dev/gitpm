## MODIFIED Requirements

### Requirement: Package folder contents
A package SHALL be a Git repository root or a folder listed by `gitpm.json`, containing `plugin.json` and output selected by `dist` (default package root). Managed deployment reads committed output; link exposes the author folder. Missing manifests SHALL fail with `invalid_manifest` naming the file. Missing-output refusal and messages follow dist ("A revision without dist is refused").

#### Scenario: Missing plugin.json
- **WHEN** a release commit has no `plugin.json`
- **THEN** install fails with `invalid_manifest` ("missing plugin.json: …")

### Requirement: Known manifest keys
The manifest SHALL require id and version and MAY carry dist, buildCommand, buildRequired, dependencies, forkOf and replaces. IDs and versions MUST be valid; forkOf and replaces MUST name another ID and take no unknown keys. forkOf.source MUST be a Git URL or absolute path. dist defaults to ".", buildRequired to false. Build commands SHALL be descriptive only, never executed.

#### Scenario: Field shapes
- **WHEN** optional fields are supplied
- **THEN** dist is a nonempty package-relative string ("." selects the package root), buildCommand is a string, buildRequired is boolean, forkOf is {id, version, commit, source?}, and replaces is {id, version}
- **AND** forkOf and replaces versions follow package-ids "Version format"

#### Scenario: Minimal manifest
- **WHEN** `plugin.json` is `{"id": "acme/greeter", "version": "0.1.0"}`
- **THEN** it is a valid manifest with no dependencies and publishes the package root for both managed installation and link

#### Scenario: Invalid forkOf
- **WHEN** `forkOf.id` equals the manifest's own `id`, or `forkOf.commit` is not a 40- or 64-character hex commit, or `forkOf.version` is empty or not MAJOR.MINOR.PATCH, or `forkOf.source` is empty, relative (`foo/bar`), a remote helper (`ext::sh -c id`, `fd::5`, any `::`) or a URL of another scheme than `https`, `http`, `ssh`, `git`, `file` (or `host:path`) or with credentials
- **THEN** the manifest is refused with `invalid_manifest` (own ID, malformed commit, empty version, invalid source), or with `invalid_version` when `forkOf.version` is not MAJOR.MINOR.PATCH (rule in package-ids, "Version format")

#### Scenario: Self-replacement
- **WHEN** `replaces.id` equals the manifest's own `id`
- **THEN** the manifest is refused with `invalid_manifest` ("a package cannot replace itself")

#### Scenario: Fork manifest without source
- **WHEN** a fork's manifest has `forkOf` without `source`
- **THEN** it is valid; `check` then knows the original only from the catalog

#### Scenario: Deployment and build metadata
- **WHEN** a manifest declares `dist: "output/vtf"`, `buildCommand: "npm run build"` and `buildRequired: true`
- **THEN** these are known fields preserved through manifest rewrites; gitpm never runs the command

#### Scenario: Unsafe deployment path
- **WHEN** dist is empty, absolute, contains a parent component or a backslash
- **THEN** the manifest is refused with invalid_manifest
