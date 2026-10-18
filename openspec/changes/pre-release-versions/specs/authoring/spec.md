## MODIFIED Requirements

### Requirement: commit releases a version
`commit REPO VERSION [--package ID]` SHALL set `version` in the package's manifest, stage ALL changes of the repository, commit `Release ID VERSION` and create the release tag (package-ids, "Release tag names"), printing `ID committed VERSION (local commit and tag TAG)`. It SHALL never push. REPO MUST be the repository root and a given `--package` MUST match a package (both `invalid_argument`). Verified by `scaffold::commit_bumps_version_commits_everything_and_tags`.

#### Scenario: Invalid version
- **WHEN** the author runs `gitpm commit REPO 1.0` or `gitpm commit REPO 1.0.0+build`
- **THEN** it fails with `invalid_version` (format in package-ids) and nothing changes

#### Scenario: Wrong-typed metadata
- **WHEN** the manifest has `"tags": "ui"`
- **THEN** it fails with `invalid_manifest` (rule in plugin-metadata, "Lenient reading, strict authoring")

#### Scenario: Release one package of a multi-package repository
- **WHEN** the author runs `gitpm commit ./tilr-plugins 0.2.0 --package tilr/store`
- **THEN** it prints `tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)`

#### Scenario: Release a pre-release
- **WHEN** the author runs `gitpm commit ./hello 1.3.0-beta.1`
- **THEN** it creates the tag `acme/hello@1.3.0-beta.1`; consumers get it only when they ask for it (updates, "Pre-releases are opt-in")
