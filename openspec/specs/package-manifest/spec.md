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
The manifest MUST have id, name and version and MAY carry dist, buildCommand, buildRequired, dependencies, forkOf and replaces. IDs MUST be canonical UUIDs, name MUST be 1 to 80 ASCII letters, digits, `-` or `_` and MUST NOT start with `-`; versions MUST be valid; forkOf and replaces MUST name another ID and reject unknown keys. forkOf.source MUST be a Git URL or absolute path.

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

### Requirement: Deployment and build metadata defaults
dist defaults to "dist", buildRequired to false. Build commands SHALL be descriptive only, never executed.

Verified by `declared_output_fork_preserves_build_metadata_without_running_it` and `declared_output_rejects_invalid_fields`.

#### Scenario: Deployment and build metadata
- **WHEN** a manifest declares `dist: "output/vtf"`, `buildCommand: "npm run build"` and `buildRequired: true`
- **THEN** these are known fields preserved through manifest rewrites; gitpm never runs the command

#### Scenario: Unsafe deployment path
- **WHEN** dist is empty, absolute, contains a parent component or a backslash
- **THEN** the manifest is refused with invalid_manifest

### Requirement: Package inspection
Library, FFI and CLI SHALL expose working-tree packages of a repository, including a root package without a declared package set. Results SHALL carry ID, name, version, path, optional forkOf, dependency IDs and requirements, and installation membership by ID in an optional workspace. Inspection SHALL NOT mutate a repository or create or lock a workspace. Existing installed listing SHALL add name and installed true while retaining dependency status details and optional-field omissions.

#### Scenario: Repository packages before installation
- **WHEN** a caller inspects a repository with uninstalled original and fork packages
- **THEN** every declared package is returned with its manifest identity, name, version, path, provenance and dependency requirements, with installed false
- **AND** omitting the workspace root reports every package uninstalled
- **AND** inspection requires no built output

#### Scenario: Installed forks
- **WHEN** a caller lists a workspace containing an installed fork
- **THEN** its entry includes its name, forkOf, dependency IDs and requirements, and installed true
- **AND** repository inspection with that workspace marks the matching ID installed

### Requirement: Installed package name snapshot
Install and update SHALL store the installed manifest's name in each package record of state format 1. Installed listing SHALL read the name from that snapshot without reading the clone. A record without `name` SHALL be refused with `invalid_data` without rewriting `state.json`. Verified by `model::state_requires_core_fields`.

#### Scenario: State record without a name
- **WHEN** an operation reads a format-1 workspace state containing a package record without `name`
- **THEN** it fails with `invalid_data` (missing field `name`)
- **AND** `state.json` keeps its original bytes

### Requirement: Explicit dependency editing
Library, FFI and CLI SHALL add, update or remove a dependency of one package selected by ID in an author repository, or an installed editable Git worktree through the locked manager. Callers SHALL supply IDs and requirements explicitly. The complete resulting manifest SHALL be validated before writing. Edits SHALL preserve other fields and unknown keys, changing no other manifest, recorded state or deployed output. Successful results SHALL describe the edited package.

#### Scenario: Add, update and remove
- **WHEN** a caller adds a dependency, changes its requirement or ID, then removes it, in an author repository or installed fork
- **THEN** each successful edit appears in the selected working-tree manifest and returned description
- **AND** unrelated packages, metadata, state and deployed output remain unchanged
- **AND** no built output is required for author edits

#### Scenario: CLI editing forms
- **WHEN** a caller uses `dependency add PACKAGE DEPENDENCY REQUIREMENT`, `dependency update PACKAGE OLD NEW REQUIREMENT` or `dependency remove PACKAGE DEPENDENCY`
- **THEN** `--repo DIR` selects an author repository; without it the configured workspace selects an installed package
- **AND** update accepts the same old and new ID to change only the requirement

#### Scenario: Invalid edit leaves bytes intact
- **WHEN** a caller adds an existing dependency, updates or removes a missing dependency, selects a missing package, or updates to an occupied destination ID
- **THEN** the operation fails with invalid_argument and all manifest bytes remain unchanged
- **AND** a self-dependency fails with invalid_manifest, an invalid dependency UUID with invalid_id, and an invalid requirement with invalid_manifest, without writes

#### Scenario: Fork then inspect and choose a sibling dependency
- **WHEN** a caller forks core and cli, inspects cli's unchanged dependency on the original core, and explicitly updates its ID to the forked core with a chosen requirement
- **THEN** cli depends on the chosen fork ID and requirement while core and unrelated dependencies remain unchanged
- **AND** no release, installation or replacement is performed implicitly
