# Design

## Context

The catalog plugin index `{"schema": 2, "id", "source", "versions"}` is read with `deny_unknown_fields`; `release.json` holds the whole manifest. Before 2026-10-18 adding keys to the index was ruled out because it breaks 0.4 clients; the owner's decision of 2026-10-18 (no backward compatibility before 1.0, TILR-38) removes that constraint (TILR-15).

## Goals / Non-Goals

**Goals:** the overview shows metadata without one `release.json` request per package; the overview stays bounded; one package's broken data never hides the others.

**Non-Goals:** search, ranking, verified flags (protected catalog: TILR-34); readers for the old index.

## Decisions

- Summary lives in the catalog plugin index: `latest {version, title, description, provides, tags}`, written with the same lock and atomic rename as the index (spec `catalog` "Writers are serialized and crash-safe"); the index `schema` is bumped and other values are refused with `catalog`.
- Open, to be decided before apply: A. only the per-package index carries the summary (one request per listed package, but no `release.json`); B. directory files also carry the summaries of the packages below them, bounded like the overview (one request per level).
- A summary that disagrees with `release.json` is display-only: installs keep verifying against `release.json`.

## Risks / Trade-offs

- [Stale summary after a manual edit of release files] → generators rewrite it with the index; a broken summary only leaves that package without metadata.
- [Old catalogs stop being readable] → accepted (TILR-38); regenerate.

## Migration Plan

Regenerate every catalog with the new gitpm; README "Changed in" lists the format change as BREAKING.
