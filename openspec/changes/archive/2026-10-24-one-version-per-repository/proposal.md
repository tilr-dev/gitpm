# One version per repository

## Why
Packages in a repository are released together. Package-specific tags make release behavior depend on package count and bypass configured tag signing for multi-package releases.

## What Changes
- Share one version and one `vVERSION` release tag across every package in a repository.
- Read only repository release tags; ignore legacy `ID@VERSION` tags without compatibility handling.
- Create the single tag through `git tag -m MESSAGE`, honoring `tag.gpgSign` without an editor.
- Add packages at the current repository version without tagging; release them only through an explicit commit with a new version.
- Align authoring, explicit fork releases, discovery, catalog, pinned refs and documentation with the shared release model.
- Remove the multi-tag ref transaction and its library-owned input requirement. Preserve immutable existing tags and safe handling of retained upstream tags in fork clones.

## Impact
- Breaking change to release-tag discovery and authoring for single- and multi-package repositories.
- Existing package IDs and package-version display names stay independent of Git tag spelling.
- No migration of legacy tags and no changes to capability Purpose text or unrelated open changes.
