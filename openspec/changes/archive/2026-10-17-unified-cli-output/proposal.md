# Proposal: Unified command output, idempotent install, per-command help

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Bench backlog: `OK:` lines on some commands and not others, `install` failing on an installed ID, `gitpm init --help` answering "unknown option: --help; use --help", a dev build reporting a release version, and `null` vs `[]` inconsistencies. Changing output later breaks hosts, and 0.5 was already changing `list` JSON, so it went into 0.5 rather than 0.6 (Claude S6).

## What Changes

- Every mutating command prints `ns/name <verb> …` per package; no `OK:` lines.
- `list`: `null` for absent strings, `[]` for absent lists.
- `install` idempotent (`ID already installed (V)`, exit 0; FFI `InstallResult.already_installed`/`available`).
- `--version` appends `-dev+<hash>` in non-release builds (build.rs); `gitpm <cmd> --help`; `migrate-catalog` prints a summary.
- Released as 0.5.0 together with #11 and #13.

## Capabilities

### Modified Capabilities
- `cli`
- `install-uninstall`

## Out of scope

- `install --upgrade` (not needed once install is idempotent).

## Impact

Implemented by: [`6ea8030`](https://github.com/tilr-dev/gitpm/commit/6ea8030) feat: unify command output, idempotent install, per-command help ([#14](https://github.com/tilr-dev/gitpm/pull/14), 2026-10-17 22:54); [`60b3495`](https://github.com/tilr-dev/gitpm/commit/60b3495) release 0.5.0 ([#12](https://github.com/tilr-dev/gitpm/pull/12), 22:56).
