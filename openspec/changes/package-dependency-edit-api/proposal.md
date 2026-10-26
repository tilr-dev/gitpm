## Why
The owner chose explicit package inspection and dependency editing instead of inferring dependency replacements from fork provenance. Fork preserves dependencies, and callers need to choose which IDs and requirements to change.

## What Changes
- **BREAKING**: remove the unreleased automatic fork dependency rewrite API and command. Callers migrate to package inspection and explicit dependency edits.
- Extend installed package inspection and expose working-tree repository package inspection, including identity, name, provenance, dependencies and installation status.
- **BREAKING**: require the installed manifest's name in format-1 state records; refuse records missing it with invalid_data without rewriting state.
- Add library, FFI and CLI operations to add, update and remove a selected package dependency, validating before writing and retaining metadata.
- Update fork next steps and the CLI/FFI parity map.

## Capabilities
### New Capabilities
- None.
### Modified Capabilities
- package-manifest: inspect packages and edit dependencies explicitly.
- multi-package-repos: remove inferred dependency rewriting.
- fork-replacement: point next steps at the explicit editing API.
- ffi-bindings: expose package inspection and dependency editing.
- cli: expose package and dependency commands.
- plugin-metadata: preserve metadata during dependency edits.

## Out of scope
- Automatic dependency mapping, release, installation, replacement or building.
- Host-specific behavior, workspace storage changes beyond the required name field and remote repository editing.

## Impact
Library, UniFFI, CLI, manifests and README. Install/update store the manifest name in state; existing format-1 records without it are refused without migration. Dependency edits leave workspace state and deployed output as recorded snapshots until an explicit release/install/update operation.
