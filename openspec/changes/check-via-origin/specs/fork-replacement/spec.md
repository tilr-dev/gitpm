## MODIFIED Requirements

### Requirement: Fork metadata
The fork's manifest SHALL get `forkOf` `{id, version, commit, source}` of the forked revision (`source`: the original's recorded source) and `replaces` `{id, version}` at the forked version; metadata and unknown keys SHALL be kept (package-manifest, "Unknown keys are preserved"; plugin-metadata, "Metadata survives rewrites"). The fork SHALL be on a new branch `fork` (`fork-2`, … when taken) with attached HEAD. Verified by `fork_records_local_head_preserves_index_and_immediate_parent`.

#### Scenario: Fork manifest
- **WHEN** `acme/hello` 1.0.0, installed from `https://git.example/acme/hello.git`, is forked into namespace `me`
- **THEN** the fork's `plugin.json` has `forkOf.id` `acme/hello`, `forkOf.version` `1.0.0`, `forkOf.source` `https://git.example/acme/hello.git` and `replaces` `{"id": "acme/hello", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`

### Requirement: check shows fork upstream
For a fork, `check` SHALL add `upstream {id, base, latest, newer, source}`: the original's latest release against the version the fork started from, taken from the catalog when it lists the original, otherwise from the release tags of `forkOf.source` (`source` `catalog` or `origin`); without either, `latest` is unknown. Verified by `check_shows_fork_upstream_and_other_catalogs` and `check::fork_upstream_from_fork_of_source`.

#### Scenario: Original has a newer release
- **WHEN** `me/hello` forked `acme/hello` 1.0.0 and the catalog has `acme/hello` 1.1.0
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base`

#### Scenario: Upstream without a catalog
- **WHEN** no catalog is given and `forkOf.source` of `me/hello` has the tag `acme/hello@1.1.0`
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original at origin: 1.1.0 — newer than the fork base`
