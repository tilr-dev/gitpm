## Why

The owner confirmed that gitpm has no public releases yet. Format versions should remain 1 while unreleased formats evolve in place.

## What Changes

- **BREAKING**: read and write catalog directory files, catalog package indexes and workspace state with version 1 only. Recreate workspaces and regenerate catalogs written with other versions; no migration or compatibility reader is added.
- Keep the catalog tree and rejection of legacy monolithic catalogs.
- Document format bumps as a separate release step: once per release when the previous public release would misread the new format; no bumps before the first public release.

## Out of scope

- Release automation and milestone management.
- Changes to other active proposals, including catalog metadata indexing.
