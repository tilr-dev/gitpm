# package-manifest Specification

## Purpose

Defines the manifest `plugin.json` every package folder carries: its known keys (`id`, `version`, `dependencies`, `forkOf`, `replaces`), how it is validated, and how unknown keys survive rewrites.

## Requirements

### Requirement: Package folder contents
A package SHALL be a folder in a Git repository (the repository root, or a folder listed by `gitpm.json`) containing its manifest `plugin.json` and a committed package dist `dist/`. A revision without a manifest SHALL be refused with `invalid_manifest` naming the missing file. The refusal of a missing package dist and its messages are specified once, in dist ("A revision without dist is refused").

#### Scenario: Missing plugin.json
- **WHEN** a release commit has no `plugin.json`
- **THEN** install fails with `invalid_manifest` ("missing plugin.json: …")

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

### Requirement: Unknown keys are preserved
Single source for unknown keys: top-level keys gitpm does not know SHALL be accepted and SHALL be written back unchanged whenever gitpm rewrites the manifest (`commit`, `fork`, `fork --replace`), after the known keys and in their original order relative to each other. The metadata keys (plugin-metadata) count as unknown keys for this rule. Verified by `model::manifest_keeps_unknown_top_level_keys_through_read_and_write` and `model::manifest_keeps_the_original_order_of_unknown_keys`.

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
- **WHEN** a fork with `forkOf` and `replaces` for `vendor/base` is installed
- **THEN** consumers of `vendor/base` still resolve to `vendor/base` until `replace vendor/base --with FORK` is run
