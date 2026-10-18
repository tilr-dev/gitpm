# Proposal: Package metadata in the catalog plugin index

## Why

Since 0.5 the catalog overview (`browse`, FFI `browseCatalog`, the CLI overview command) reads the latest `release.json` of every listed package to show its metadata: up to 40 extra HTTP requests per overview on a remote catalog (README "Plugin metadata"; Claude S6). The metadata was kept out of the catalog plugin index only so that gitpm 0.4 clients, which read the index with `deny_unknown_fields`, keep working. The owner decided on 2026-10-18 that there is no backward compatibility before 1.0 (TILR-38), so the index can simply carry it (TILR-15: "add package metadata to the catalog so a host can show title/provides/tags without cloning"). Status: verified not implemented (`catalog.rs` `PluginIndex` has `schema`, `id`, `source`, `versions` only).

## What Changes

- **BREAKING** (catalog format) The catalog plugin index `namespaces/<ns>/<name>/plugin.json` gains the metadata of the package's latest release (`title`, `description`, `provides`, `tags`) and its version; its `schema` value is bumped, and readers refuse other values as today. Migration: regenerate the catalog with the catalog generation command; no reader of the old index is kept.
- Catalog generation and registration write that summary in the same locked, atomic write as the index.
- The catalog overview reads the summary from the index instead of one `release.json` per package. The two guarantees stay: the overview is bounded (40 entries), and missing or broken data of one package only leaves that package without metadata.
- Open design question (design.md): whether directory files also carry the summaries of the packages below them (one request per level) or only the per-package index does.

## Capabilities

### Modified Capabilities
- `catalog`: "Catalog v2 is a tree of static files" (index format and schema).
- `plugin-metadata`: "Metadata in the catalog" (overview reads the index).

## Out of scope

- Compatibility with gitpm 0.4/0.5 readers (TILR-38).
- Search index, snapshots or Parquet exports; host-side slot UI.
- Changing `release.json`, which stays the source of truth for installs.
- The CLI command names (change `cli-catalog-commands`, TILR-8); this change names operations, not commands.

## Impact

`crates/gitpm/src/catalog.rs` (index type, writers, `browse`), catalog file layout, README "Plugin metadata" and the catalog format notes, FFI `PluginSummary` unchanged. Tracker: TILR-15.
