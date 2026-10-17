# Design

## Context

Recorded constraint (README "Plugin metadata"): the plugin index `{"schema": 2, "id", "source", "versions"}` is parsed with `deny_unknown_fields` by 0.4, so adding keys to it breaks 0.4 clients; bumping `schema` to 3 makes 0.4/0.5 refuse the index ("unsupported plugin schema"). Directory files have the same strictness.

## Goals / Non-Goals

**Goals:** one request per overview level on HTTPS catalogs; older clients unaffected.

**Non-Goals:** search, ranking, verified flags.

## Decisions

Open, to be decided before apply (no decision recorded):
- A. Sidecar file per plugin (for example `namespaces/<ns>/<name>/summary.json`) holding latest-release metadata; index unchanged. Old clients never read it. Cost: still one request per plugin unless aggregated.
- B. Aggregated sidecar per directory level (for example `summary.json` beside each directory `catalog.json`) listing metadata of the plugins below it, bounded like the overview (40). One request per level.
- C. Schema 3 index with metadata; requires a coordinated client upgrade (rejected direction so far: breaks 0.4/0.5).

## Risks / Trade-offs

- [Stale sidecar after a manual edit of release files] → generators rewrite sidecars in the same locked, atomic write as the index.
- [Sidecar disagrees with `release.json`] → `release.json` stays the source of truth for install; the sidecar is display-only.
