# package-manifest Specification

## Purpose

Defines the manifest `plugin.json` every package folder carries: its known keys (`id`, `version`, `dependencies`, `forkOf`, `replaces`), how it is validated, and how unknown keys survive rewrites.

## Requirements

### Requirement: Package folder contents
A package SHALL be a folder in a Git repository (the repository root, or a folder listed by `gitpm.json`) containing its manifest `plugin.json` and a committed package output folder selected by `dist` (default `dist/`). A revision without a manifest SHALL be refused with `invalid_manifest` naming the missing file. The refusal of a missing package dist and its messages are specified once, in dist ("A revision without dist is refused").

#### Scenario: Missing plugin.json
- **WHEN** a release commit has no `plugin.json`
- **THEN** install fails with `invalid_manifest` ("missing plugin.json: …")

### Requirement: Known manifest keys
The manifest SHALL require id, name and version and MAY carry dist, buildCommand, buildRequired, dependencies, forkOf and replaces. IDs MUST be canonical UUIDs, name MUST be 1 to 80 ASCII letters, digits, `-` or `_` and MUST NOT start with `-` and versions MUST be valid; forkOf and replaces MUST name another ID and take no unknown keys. forkOf.source MUST be a Git URL or absolute path. dist defaults to "dist", buildRequired to false. Build commands SHALL be descriptive only, never executed.

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

#### Scenario: Deployment and build metadata
- **WHEN** a manifest declares `dist: "output/vtf"`, `buildCommand: "npm run build"` and `buildRequired: true`
- **THEN** these are known fields preserved through manifest rewrites; gitpm never runs the command

#### Scenario: Unsafe deployment path
- **WHEN** dist is empty, absolute, contains a parent component or a backslash
- **THEN** the manifest is refused with invalid_manifest

#### Scenario: Legacy manifest refused
- **WHEN** a manifest has a scoped ID or omits its name
- **THEN** it is refused without migrating or rewriting it

### Requirement: Unknown keys are preserved
Single source for unknown keys: top-level keys gitpm does not know SHALL be accepted and SHALL be written back unchanged whenever gitpm rewrites the manifest (`commit`, `fork`), after the known keys and in their original order relative to each other. The metadata keys (plugin-metadata) count as unknown keys for this rule. Verified by `model::manifest_keeps_unknown_top_level_keys_through_read_and_write` and `model::manifest_keeps_the_original_order_of_unknown_keys`.

#### Scenario: Host metadata survives a release
- **WHEN** the manifest contains a host-specific key `"main": "index.js"` and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the rewritten `plugin.json` has `version` `0.2.0` and still contains `"main": "index.js"`, written after the known keys

### Requirement: Manifest output format
Manifests written by gitpm SHALL be pretty-printed JSON with a trailing newline and a stable field order (known keys first, then unknown keys). `"dependencies"` SHALL always be written, as `{}` when there are none; `forkOf` and `replaces` only when set. Verified by `model::manifest_json_is_pretty_with_trailing_newline_and_stable_fields`.

#### Scenario: Rewritten manifest
- **WHEN** gitpm writes `plugin.json`
- **THEN** the file ends with a newline and lists `id`, `version` before the kept keys

### Requirement: forkOf is provenance, replaces is an offer
`forkOf` SHALL only record where a fork came from; it MUST NOT redirect any consumer. `replaces` SHALL declare that the package can stand in for another ID at a version, and it SHALL take effect only after an explicit replace (see fork-replacement).

#### Scenario: Installing a fork changes no consumer
- **WHEN** a fork with `forkOf` and `replaces` for `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` is installed
- **THEN** consumers of `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` still resolve to `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` until `replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with FORK` is run
