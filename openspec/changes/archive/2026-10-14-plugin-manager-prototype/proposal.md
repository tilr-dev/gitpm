# Proposal: Plugin manager prototype (`pm`)

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Tilr needed a way to install, update and roll back plugins that live in Git repositories. The owner wanted to try install/uninstall in a standalone prototype before embedding anything in the app; plugins are installed and developed only on desktop (iOS never runs the manager). Source: Codex session 2026-10-14 20:48 (cwd `~/projects/tilr`), Basic Memory note "Tilr — менеджмент плагинов: use cases" (UC1–UC14).

## What Changes

- Rust library + CLI `pm` in `~/projects/tilr/temp-plugin-manager` (outside this repository; no commits here).
- Install model: plugin = Git clone under `plugins/` plus a copy of the repository's ready `dist/` into `dist/<id>/`; nothing is built.
- Operations prepared a new workspace copy first, so a failed install/update left old files and state intact; local edits and own commits are never overwritten.
- Commands: install, update, rollback, uninstall, init/create, remote, commit (bump version, commit, tag), register, check (renamed from `outdated`), fork.
- IDs became `namespace/name` with scoped tags `namespace/name@VERSION`; legacy unscoped IDs keep `vVERSION`.
- Fork lives in the forker's namespace with `forkOf {id, version, commit}` as provenance only.
- Automatic `pinned` on catalog/--ref installs was removed entirely at the owner's request; the exact commit is always stored.
- One catalog, hosted as static files at pkg.tilr.dev on an R2 bucket with no Worker and no publish API; `pm sync` uploaded and purged the Cloudflare cache.

## Capabilities

### Modified Capabilities
- `package-ids`
- `install-uninstall`
- `updates`
- `authoring`

## Out of scope

- Dependencies and the `replaces` mechanism (deferred "together with the dependency solver").
- Build step inside the manager.
- Embedding into the Tilr app.

## Impact

Implemented by: No gitpm commits (prototype repository). Codex rollout `2026/10/14/rollout-2026-10-14T20-48-08-…`.
