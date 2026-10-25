## MODIFIED Requirements

### Requirement: Metadata survives rewrites
`commit` and `fork` SHALL rewrite `version`, `id`, `forkOf`, `replaces` and dependencies but SHALL keep the metadata keys exactly as they were, in the position the unknown-key rule gives them (package-manifest, "Unknown keys are preserved"). (The `"title": "NAME"` that `init` writes is specified in authoring.) Verified by `fork_and_commit_carry_the_metadata_unchanged`.

#### Scenario: Fork keeps the title
- **WHEN** `f851f609-bb81-52d6-bfef-33798624782c` with `"title": "Theme"` is forked as `d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50`
- **THEN** the fork's manifest still has `"title": "Theme"`

### Requirement: Filter by provided service
`list --provides SERVICE` (library `list_with`, FFI `list`) SHALL keep only the packages whose `provides` contains `SERVICE` in `packages` and `resolved`; `replacements` and `forgotten` SHALL be printed unfiltered, and inactive (replaced, disabled) providers SHALL be kept because `list` is a status view. The filter SHALL be applied by the library.

#### Scenario: Replaced original still listed
- **WHEN** `d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50` replaces `f851f609-bb81-52d6-bfef-33798624782c`, both providing `theme`
- **THEN** `list --provides theme` lists both, with `active` false for `f851f609-bb81-52d6-bfef-33798624782c`

### Requirement: Metadata in the catalog
`catalog update` and `catalog register` SHALL publish the latest release's metadata in the package index, under the catalog lock in the same atomic write. `catalog show` and `browse` SHALL read that summary without reading `release.json`, with at most one index request per listed package and the existing bounded overview. A missing or broken summary SHALL only leave its package without metadata. Installs SHALL keep verifying against `release.json`.

#### Scenario: info shows metadata
- **WHEN** the latest release of `f851f609-bb81-52d6-bfef-33798624782c` has a title and provides
- **THEN** `gitpm catalog show` prints `Title: Theme` and `Provides: theme` under the package
- **AND** this is covered by `catalog_releases_carry_and_show_the_metadata`

#### Scenario: Overview without per-package release reads
- **WHEN** an HTTPS catalog lists 40 packages
- **THEN** `browse` shows their metadata with at most 40 index requests and without requesting any `release.json`

#### Scenario: One broken summary
- **WHEN** the index summary of one listed package is missing or invalid
- **THEN** the overview shows that package without metadata and the others normally
