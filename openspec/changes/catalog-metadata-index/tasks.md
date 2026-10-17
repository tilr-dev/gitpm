# Tasks

## 0. Decision

- [ ] 0.1 Choose option A or B in design.md with the owner and record it

## 1. Catalog writer

- [ ] 1.1 Write the metadata sidecar in `catalog-update`, `register` and `migrate-catalog` under the catalog lock; unit test asserts it matches the latest release manifest and that a 0.5-format reader still parses the index (`deny_unknown_fields` paths)
- [ ] 1.2 Keep immutability and atomicity: a rejected update writes no sidecar; verify with the existing tree-hash assertions

## 2. Reader and docs

- [ ] 2.1 `browse` uses the sidecar when present and falls back to `release.json`; test counts HTTP requests against a loopback server (≤ 1 per level with option B)
- [ ] 2.2 Update README "Plugin metadata" (cost paragraph) and the catalog format description
