# Proposal: Catalog v2 tree, generation from Git tags, single catalog

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

A single `catalog.json` read and rewritten as a whole does not scale (the owner raised very large plugin counts) and the owner wanted to rely on Cloudflare static hosting with little own traffic. Merging several catalogs APT-style was considered and rejected to avoid ID conflicts: one catalog, which the user may replace with their own. Source: Codex sessions 2026-10-15 00:15 and 09:09.

## What Changes

- Catalog v2: a tree of static JSON files holding only links (`catalog.json` → namespace directories → plugin index → `versions/<v>/release.json`).
- `release.json` records the manifest, source, commit, reference, SHA-256 and size of every `dist/` file, and an optional build `url` + `sha256` (format only; installs still come from Git).
- Hashes are verified after checking out the pinned commit; a mismatch aborts before the workspace changes.
- Git is the source of releases: `sources.txt` → `catalog-update` reads release tags and writes the tree; a moved tag of a published version is a conflict and the whole update is rejected.
- `install GIT_URL`/`update` pick the newest release tag by version number; `--channel dev` follows the default branch explicitly.
- `info` (catalog overview), `--catalog <dir|https>`, `auth check GIT_URL` (git ls-remote only, never stores secrets).
- Decision: exactly one selected catalog per workspace; no merging, priorities or per-install pinning. The Python HTTP catalog was removed.

## Capabilities

### Modified Capabilities
- `catalog`
- `git-safety`

## Out of scope

- Search index, snapshots, Parquet export, downloading prebuilt archives (discussed, not built).
- Namespace ownership / verified publishers.

## Impact

Implemented by: No gitpm commits (prototype repository). Later carried into the library by 9d21a20.
