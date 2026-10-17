# Proposal: Configurable layout and tag scheme

## Why

When gitpm was made host-agnostic for 0.1.0, these were deferred to "0.2+" (Claude S4; Basic Memory note "gitpm — крейт на crates.io…", "[todo] 0.2"): a `Layout`/`Options` with `manifest_file`, `payload_dir` (`None` = whole repository without `dist/`), `repos_dir`, `state_file`, `txn_dir`, the lock file inside the root, `git_config_prefix`; a configurable `TagScheme`; scaffold templates; renaming `plugin` to `package` in the API (`InstalledPlugin`). Status: verified not implemented — `Options` has only `catalog`; `plugin.json`, `dist/` and tag formats are fixed. The note is from 2026-10-16 and may be stale; whether any host needs this was not recorded.

## What Changes

To be decided item by item; nothing is committed to yet.

## Capabilities

### Modified Capabilities
- Potentially `workspace-layout`, `package-manifest`, `package-ids`, `dist` — deltas only after the owner picks items; `skip_specs: true`.

## Out of scope

- Breaking the default layout: defaults must stay as today.

## Impact

Library `Options`, workspace paths, scaffold, FFI records, README.
