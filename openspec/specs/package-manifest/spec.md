# package-manifest Specification

## Purpose

Defines `plugin.json`, the manifest every package folder carries: its known keys (`id`, `version`, `dependencies`, `forkOf`, `replaces`), how it is validated, and how unknown keys survive rewrites.

## Requirements

### Requirement: Package folder contents
A package SHALL be a folder in a Git repository (the root, or a folder listed by `gitpm.json`) containing a `plugin.json` file and a committed `dist/` folder. A revision without `plugin.json` SHALL be refused with `invalid_manifest` naming the missing file. The refusal of a missing `dist/` and its messages are specified once, in dist ("A revision without dist is refused").

#### Scenario: Missing plugin.json
- **WHEN** a release commit has no `plugin.json`
- **THEN** install fails with `invalid_manifest` ("missing plugin.json: …")

### Requirement: Known manifest keys
`plugin.json` SHALL require `id` (a valid package ID) and `version` (format in package-ids, "Version format") and MAY carry `dependencies` (see dependencies), `forkOf` `{id, version, commit}` and `replaces` `{id, version}` without unknown keys; their IDs MUST be valid and other than the own `id`, their versions MUST pass the version format. Verified by `model::manifest_parses_minimal_and_fork_manifests` and `model::manifest_rejections_have_precise_kinds`.

#### Scenario: Minimal manifest
- **WHEN** `plugin.json` is `{"id": "acme/greeter", "version": "0.1.0"}`
- **THEN** it is a valid manifest with no dependencies

#### Scenario: Invalid forkOf
- **WHEN** `forkOf.id` equals the manifest's own `id`, or `forkOf.commit` is not a 40- or 64-character hex commit, or `forkOf.version` is empty or not MAJOR.MINOR.PATCH
- **THEN** the manifest is refused (`invalid_manifest` or `invalid_version`)

#### Scenario: Self-replacement
- **WHEN** `replaces.id` equals the manifest's own `id`
- **THEN** the manifest is refused with `invalid_manifest` ("a plugin cannot replace itself")

### Requirement: Unknown keys are preserved
Top-level keys gitpm does not know SHALL be accepted and SHALL be written back unchanged and in their original order whenever gitpm rewrites the manifest (`commit`, `fork`). Verified by `model::manifest_keeps_unknown_top_level_keys_through_read_and_write` and `model::manifest_keeps_the_original_order_of_unknown_keys`.

#### Scenario: Host metadata survives a release
- **WHEN** `plugin.json` contains a host-specific key `"main": "index.js"` and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the rewritten `plugin.json` has `version` `0.2.0` and still contains `"main": "index.js"` at its original position

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
