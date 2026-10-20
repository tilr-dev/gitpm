# Design

## Context

The catalog package index `{"schema": 2, "id", "source", "versions"}` is read with `deny_unknown_fields`; `release.json` holds the whole manifest. Before 2026-10-18 adding keys to the index was ruled out because it breaks 0.4 clients; the owner's decision of 2026-10-18 (no backward compatibility before 1.0, TILR-38) removes that constraint (TILR-15).

## Goals / Non-Goals

**Goals:** the overview shows metadata without one `release.json` request per package; the overview stays bounded; one package's broken data never hides the others.

**Non-Goals:** search, ranking, verified flags (protected catalog: TILR-34); readers for the old index; summaries in directory files.

## Decisions

- Summary lives in the catalog package index: `latest {version, title, description, provides, tags}`, written with the same lock and atomic rename as the index (spec `catalog` "Writers are serialized and crash-safe"); the index `schema` becomes 3 and other values are refused with `catalog`.
- Only the per-package index carries the summary (owner decision of 2026-10-18 on TILR-15, variant A): one index request per listed package, no `release.json`; directory files stay links only. No reader for schema-2 indexes is kept (TILR-38).
- A summary that disagrees with `release.json` is display-only: installs keep verifying against `release.json`.

## Risks / Trade-offs

- [Stale summary after a manual edit of release files] → generators rewrite it with the index; a broken summary only leaves that package without metadata.
- [Old catalogs stop being readable] → accepted (TILR-38); regenerate.

## Migration Plan

Regenerate every catalog with the new gitpm; README "Changed in" lists the format change as BREAKING.
