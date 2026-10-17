# Proposal: dist/ holds only active packages

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

After seeing both `dist/acme/greeter` and `dist/me/greeter`, the owner: "in dist I should have only installed ones; dist must always be the current state" and "dist = all needed plugins; interfaces are sorted out one level up". Two sources of truth (dist = everything, state says what is effective) forced every host to resolve replacements itself (Claude S6; Basic Memory design note 2026-10-17). Copying fork files under the original ID was rejected.

## What Changes

- `dist/<id>` exists iff installed, enabled and not replaced; `replace` removes the original's dist, `--back`/`enable` redeploy from the clone at the installed commit; `disable`/`forget` remove.
- Every mutating command reconciles the whole `dist/` (unknown folders reported, never removed); read-only commands do not touch it.
- `gitpm sync-dist [--dry-run]` (FFI `syncDist`/`syncDistPlan`, `DistSync`) repairs 0.4 workspaces.
- Deploys use one `git archive` of the committed tree (new `tar` dependency); a release without `dist/` is an error.
- `replace --with` a disabled fork refused; disabling a replacer needs `--force`; `resolved[].active` and FFI `InstalledPlugin.active`.

## Capabilities

### Modified Capabilities
- `dist`
- `fork-replacement`
- `ffi-bindings`

## Out of scope

- Semantic deduplication by `provides` (host concern).
- Generating `plugins.json` or load order (host concern).

## Impact

Implemented by: [`3819836`](https://github.com/tilr-dev/gitpm/commit/3819836) feat: dist holds only active packages ([#13](https://github.com/tilr-dev/gitpm/pull/13), 2026-10-17 22:52).
