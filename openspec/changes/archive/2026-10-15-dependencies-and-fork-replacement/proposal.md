# Proposal: Dependencies with PubGrub, dependent guards, global fork replacement

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Use cases UC3/UC11 (dependencies resolved with PubGrub), UC8 (warn before removing a package others depend on) and UC12 (show a fork's upstream version) were unmet; on 2026-10-15 the owner made fork replacement mandatory (missing implementation = FAIL). Decisions (Claude S1): replacement is global ("globally the fork is used everywhere"), compatibility is checked by declared versions only, with a Cargo-like "warn and ask" policy (variant B): TTY asks [y/N], otherwise refuse unless --force; self-replacement and cycles always refused.

## What Changes

- Manifest `dependencies` with Cargo-style requirements and optional `replaces {id, version}`.
- PubGrub resolution over installed (fixed) packages plus the catalog; install pulls missing dependencies in one transaction.
- Update/rollback refuse versions that break dependents (`dependency_conflict`); state keeps edges.
- disable/forget/uninstall refuse with `has_dependents` unless forced.
- `replace`/`switch_back` with plan + confirm (`incompatible_replacement`); one catalog per workspace enforced (`plugins.<id>.catalog`).
- `check` reports a fork's upstream (`upstream {id, base, latest, newer}`) and `other_catalog`; FFI gained dependency and replacement calls.

## Capabilities

### Modified Capabilities
- `dependencies`
- `fork-replacement`
- `catalog`

## Out of scope

- Per-consumer replacement (rejected).
- Automatic merge of upstream into forks.

## Impact

Implemented by: [`1421b6a`](https://github.com/tilr-dev/gitpm/commit/1421b6a) Dependencies with PubGrub, dependent guards and global fork replacement (2026-10-15 21:52); [`1ed09d9`](https://github.com/tilr-dev/gitpm/commit/1ed09d9) check: fork upstream version and other-catalog status; FFI for deps and replace (21:54).
