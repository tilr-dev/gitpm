# Design

## Context

The package index contains identity, source, and version links. Release records contain the complete manifest. The overview needs only descriptive metadata.

## Decisions

- Add `latest {version, title, description, provides, tags}` to the package index without changing its schema number. Select the newest stable version, or the newest prerelease if no stable version exists, matching the overview's existing metadata selection.
- Write the summary under the existing catalog lock in the same atomic rename as version links, including when an existing index is regenerated.
- Fetch at most one index per listed package and no release records. Treat a missing, malformed, or mismatched summary as absent metadata for that package.
- Summary data is display-only. Install validation continues to use release records.

## Migration

Regenerate existing catalogs with `gitpm catalog update`. No fallback to release reads is provided.
