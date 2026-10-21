# Proposal: Package metadata in the catalog package index

## Why

The catalog overview reads the latest `release.json` of every listed package to show metadata, adding up to 40 HTTP requests per remote overview. The package index can carry this display summary directly. Backward compatibility with older catalogs is not required.

## What Changes

- The catalog package index gains `latest` with `version`, `title`, `description`, `provides`, and `tags`. Its schema number stays unchanged. Regenerate existing catalogs with `gitpm catalog update`.
- Generation and registration publish the summary under the catalog lock in the same atomic write as the index.
- `catalog show`, `browse`, and FFI `browseCatalog` read the summary without fetching `release.json`. The 40-entry limit and isolation of missing or broken summaries remain.
- Directory files remain links only; installs continue to verify `release.json`.

## Capabilities

### Modified Capabilities
- `catalog`: "Catalog v2 is a tree of static files".
- `plugin-metadata`: "Metadata in the catalog".

## Out of scope

Older catalog compatibility, directory summaries, search, and changes to the install release format or FFI `PackageSummary`.
