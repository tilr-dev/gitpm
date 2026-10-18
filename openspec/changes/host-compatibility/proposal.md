# Proposal: Host compatibility ranges in the manifest

## Why

The owner decided on 2026-10-18 (decision D10, TILR-42) that a package declares which host versions it works with, like VS Code `engines`: `"host": {"<name>": "<range>"}`. Today gitpm has no such field, so a host that upgrades its plugin API cannot stop an older host from installing a package built for the new one, and `update all` can move a package past what the running host supports. The library stays host-agnostic: it knows no host names and only compares the range with the host the caller passes.

## What Changes

- Manifest key `host` `{"<name>": "<range>"}` (ranges in the dependency syntax of `pre-release-versions`), validated with `invalid_manifest`, written back unchanged; reported in `list`, catalog summaries and FFI records (`{}` when absent).
- `Options.host` `{name, version}` (CLI `--host NAME@VERSION` / `GITPM_HOST`; FFI `ManagerOptions.host`). Without it nothing is filtered.
- Install, `install-catalog`, `download`, update and `update all` pick the newest compatible release; none, or an explicit ref/version that does not fit, fails with the new error kind `incompatible_host`; `update all` keeps the package back with reason `needs NAME RANGE`.
- `check` notes `V needs NAME RANGE` for a newer release that does not fit, instead of offering it; the catalog release record carries `host` inside its manifest.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`, `removal-and-restore-rules`, `no-tag-immutability`, `protected-catalog`, `pre-release-versions` (range syntax).

## Capabilities

### Modified Capabilities
- `package-manifest`: new "Host compatibility field".
- `plugin-metadata`: "Reported shape" (`host`).
- `updates`: new "Releases are chosen for the host".
- `install-uninstall`: new "Install respects the host".
- `catalog`: new "Host ranges in the catalog".
- `errors`: "Stable error kinds" (`incompatible_host`).
- `cli`: "Settings come from flags, environment, defaults" (`--host`, `GITPM_HOST`).
- `ffi-bindings`: new "Host in ManagerOptions".

## Out of scope

- Any host-specific name or semantics in the library (Tilr chooses its own name and version).
- Compatibility between packages beyond `dependencies`.
- Indexing host ranges in the catalog plugin index (`catalog-metadata-index`).

## Impact

`model.rs` (manifest `host`), `workspace.rs`/`upgrade.rs` selection, `check.rs`, `error.rs` (`IncompatibleHost`, README kinds list), CLI settings, FFI `ManagerOptions`/`PmError`, README manifest and settings sections, "Changed in 0.6".
