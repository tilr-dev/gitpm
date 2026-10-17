# Proposal: Pre-release versions

## Why

`update all` was built for stable releases only, and "pre-release versions in `update all`" was listed as not implemented when it shipped (Codex session 2026-10-16 20:52). Verified on `dev`: pre-releases are refused everywhere, not only in `update all` — `catalog::version_key` accepts only `MAJOR.MINOR.PATCH`, and `deps::requirement` refuses pre-release comparators. Rationale for wanting pre-releases (who needs them, which channel) was not recorded.

## What Changes

To be decided; candidate scope:
- Accept `MAJOR.MINOR.PATCH-PRE` in manifests, tags and catalogs, ordered by SemVer.
- Never select a pre-release implicitly: stable `install`/`update`/`update all` ignore them unless opted in (flag or explicit requirement), as Cargo does.

## Capabilities

### Modified Capabilities
- `package-ids` (version format), `dependencies` (requirements), `updates` (selection) — deltas to be written once the semantics are decided; this change carries `skip_specs: true` until then.

## Out of scope

- Build metadata (`+build`) in versions.
- Changing the dev channel (commit identity stays).

## Impact

Version parsing in `catalog.rs`/`deps.rs`, release discovery, catalog compatibility for older clients (a 0.5 client refuses such versions), README.
