# Proposal: Plugin metadata and CLI polish

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Tilr needs to group installed plugins (for example a theme dropdown) without cloning or running JavaScript; slot semantics stay in the host, gitpm only stores and reports `provides`/`tags` (Basic Memory note "Tilr — слоты плагинов"; Claude S6). Bench friction (0.4 backlog) asked for `init --multi`, `init --no-release`, install lines naming the package, `clone` in `resolved[]`, a clearer `replace --back` message and no empty folders.

## What Changes

- `plugin.json` `title`, `description`, `provides`, `tags` stored in state and exposed by `list` (`resolved[]`, `plugins.<id>.metadata`), `list --provides SERVICE`, `info`/`browse`, FFI records.
- Lenient reading (wrong types ignored with a warning), strict only on `commit`.
- Catalog index format unchanged (0.4 reads it with `deny_unknown_fields`); `browse` reads each latest `release.json` (at most 40).
- `init --multi`, `init --no-release`; install/update print package and version; `update all` one line per package; `replace --back` message; `clone` in `resolved[]`; empty `dist/<ns>` and `repos/` parents removed.

## Capabilities

### Modified Capabilities
- `plugin-metadata`
- `authoring`
- `cli`

## Out of scope

- Slot/exclusivity logic (host side).
- Metadata in the catalog index (see change `catalog-metadata-index`).

## Impact

Implemented by: [`6c2abf1`](https://github.com/tilr-dev/gitpm/commit/6c2abf1) feat: plugin metadata and CLI polish ([#11](https://github.com/tilr-dev/gitpm/pull/11), 2026-10-17 21:40).
