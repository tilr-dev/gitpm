# Proposal: `download` and a strict `enable`; disabled packages satisfy no dependency

## Why

The owner decided on 2026-10-18 (decision D7, TILR-36) that a disabled package does not satisfy dependencies anywhere, that a new `download ID` fetches a package (and its dependencies) without enabling it, that `enable` refuses while dependencies are not active (`--force` overrides) and that `install` = download + enable. Today the install/update solver counts a disabled package as satisfying (review finding G2#5), while `update all`, `list` and `check` count it as unsatisfied (spec `dependencies` "Disabled packages and dependencies" describes this split as current behavior).

## What Changes

- New `download SOURCE [ID] [--package ID] [--ref REF | --dev]` and `download-catalog ID [--version V]` (library `Manager::download`, `download_from_catalog`; FFI `download`, `downloadFromCatalog`): clone, verify and record with `enabled: false`; missing dependencies are downloaded disabled; nothing is deployed.
- `install`/`install-catalog` = download + enable of the package and the dependencies it pulled, in one transaction.
- **BREAKING** A disabled package never satisfies a dependency: install/enable/update of an enabled consumer fails with `dependency_conflict` ("DEP is disabled").
- **BREAKING** `enable ID` refuses with `dependency_conflict` when a dependency is not satisfied by an active package; `enable --force` (FFI `enable(force)`) enables anyway and leaves the consumer reported broken. `enable` never enables dependencies itself.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`.

## Capabilities

### Modified Capabilities
- `install-uninstall`: new "Download without enabling"; "Install the latest release of a Git source".
- `dependencies`: new "enable needs active dependencies"; "Disabled packages and dependencies".
- `dist`: "Inactive packages change state and clone only", "Enable and disable deploy and remove", "Every mutating command reconciles all of dist".
- `cli`: "Command set", "Option validation", "One output line per affected package".
- `ffi-bindings`: "Manager surface".

## Out of scope

- Automatic enabling of dependencies by `enable` (see design.md).
- Host activation policies (which provider of a service is enabled): host concern.

## Impact

`workspace.rs` (download paths, enable check), `deps.rs` (only active providers satisfy), CLI commands and lines, FFI methods, README commands and "Changed in 0.6", Dagger cases for download/enable.
