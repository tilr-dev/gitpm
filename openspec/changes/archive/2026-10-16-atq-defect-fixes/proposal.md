# Proposal: Fixes from the ATQ and Dagger acceptance runs

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

The ATQ integration run (68 use cases, 2026-10-15/16) failed 26 cases (Claude S3; Codex logs list UC-003, 014, 018/043, 022, 041, 045, 061, 062, 065 among them), and the Dagger port reported 890 pass / 15 fail (Codex 2026-10-16 15:48). P0 was writes and deletes outside the workspace through symlinked namespace folders. The owner explicitly postponed the HTTP response size limit.

## What Changes

- Writes through symlinked `plugins/<ns>` or `dist/<ns>` refused.
- update/rollback refuse stash and unpublished local branches/tags; the uninstall prompt lists the state entry.
- update mirrors upstream tags (prune, force): a moved tag of a seen release is `version_conflict`.
- `check` on an other-catalog plugin; error messages name the ID; missing `plugin.json` is `invalid_manifest`.
- Release immutability: known release tags compared before/after fetch; a scoped tag whose manifest ID differs is a corrupted release (inherited tags of a fork still supported).
- Case collisions compared on real directory names, independent of the file system.
- Fork update: a published tag of the original no longer counts as unpublished local work.
- CLI `check` goes through the library; catalog diagnostics improved.

## Capabilities

### Modified Capabilities
- `transactions-locking`
- `updates`
- `errors`
- `package-ids`
- `catalog`

## Out of scope

- Streaming HTTP size limit (owner: do not fix now; still open, see change `catalog-streaming-size-limit`).

## Impact

Implemented by: [`4bdbd7a`](https://github.com/tilr-dev/gitpm/commit/4bdbd7a) Fix ATQ run defects: managed-path symlinks, hidden local work, tag sync (2026-10-16 10:59); [`438d3a1`](https://github.com/tilr-dev/gitpm/commit/438d3a1) Fix release immutability, fork updates and catalog diagnostics (16:38).
