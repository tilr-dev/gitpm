## MODIFIED Requirements

### Requirement: Metadata survives rewrites
`commit` and `fork` rewrite version and identity fields; `fork` SHALL preserve dependency IDs and requirements, and explicit `fork-rewrite` SHALL rewrite only dependencies. All three operations SHALL keep the metadata keys exactly as they were, in the position the unknown-key rule gives them (package-manifest, "Unknown keys are preserved"). (The `"title": "NAME"` that `init` writes is specified in authoring.) Verified by `fork_and_commit_carry_the_metadata_unchanged`.

#### Scenario: Fork keeps the title
- **WHEN** `f851f609-bb81-52d6-bfef-33798624782c` with `"title": "Theme"` is forked as `d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50`
- **THEN** the fork's manifest still has `"title": "Theme"`

#### Scenario: Explicit rewrite keeps metadata
- **WHEN** the explicit dependency rewrite changes a fork manifest containing metadata and unknown keys
- **THEN** those keys and values are retained unchanged
- **AND** this is covered by `explicit_rewrite_validates_all_manifests_and_preserves_external_dependencies_and_metadata`
