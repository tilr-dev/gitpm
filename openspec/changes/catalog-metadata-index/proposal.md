# Proposal: Plugin metadata in the catalog index

## Why

Since 0.5 `info`/`browse` read the latest `release.json` of every listed plugin to show metadata: up to 40 extra HTTP requests per overview on a remote catalog. The metadata could not go into the plugin index because gitpm 0.4 reads it with `deny_unknown_fields`, so a "catalog schema 3" or a cache was deferred (Claude S6; README "Plugin metadata"; bench TODO.txt "каталог: метаданные в индексе plugin.json (schema 3)"). The slots design note also wanted the catalog to expose `provides`/`tags` so uninstalled providers are visible. Status: verified not implemented (catalog.rs `PluginIndex` has `schema`, `id`, `source`, `versions` only).

## What Changes

- `catalog-update`/`register`/`migrate-catalog` publish each plugin's latest-release metadata (`title`, `description`, `provides`, `tags`) next to the plugin index, in a form older clients ignore.
- `browse`/`info`/`PluginSummary` use it when present and fall back to reading `release.json`.

## Capabilities

### Modified Capabilities
- `plugin-metadata`: catalog overview reads published metadata instead of one `release.json` per plugin.

## Out of scope

- Search index, snapshots or Parquet exports.
- Host-side slot UI.
- Changing `release.json`.

## Impact

`crates/gitpm/src/catalog.rs` (writers, `browse`), catalog file layout (additive), README "Plugin metadata" and catalog format notes, FFI `PluginSummary` unchanged.
