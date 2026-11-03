## MODIFIED Requirements

### Requirement: Installed package record
Each `state.json` `packages.<id>` managed record SHALL hold `source`, `name`, `current`, at most one `previous`, `enabled`, and optional `mode`, `forkOf`, `dependencies`, `replaces`, `catalog`, `clone`, `path`, `metadata`. Revisions hold `version` and `commit`; `previous` retains mode. Links follow "Link record format". Git worktree paths SHALL derive from UUID; `path` selects the package folder inside that tree. Builds SHALL own no worktree. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

#### Scenario: Recorded modes
- **WHEN** packages were installed without a ref, with `--ref v1.0.0`, and with `--ref feature`
- **THEN** the first record has no `mode`, the second `"mode": {"pinned": "v1.0.0"}`, the third `"mode": {"branch": "feature"}`

#### Scenario: Previous revision keeps its mode
- **WHEN** a package in branch mode `main` is updated with `--release`
- **THEN** `previous` is `{"version": …, "commit": …, "mode": {"branch": "main"}}` (a `previous` in release mode has no `mode`), so `rollback` restores both

#### Scenario: Previous build provenance
- **WHEN** a build revision is updated to a Git revision
- **THEN** previous_build retains the release metadata and catalog location for offline rollback

#### Scenario: Build record format
- **WHEN** a current or previous revision is a build
- **THEN** `build` or `previous_build` retains its full release record and catalog location separately from catalog protection; previous retains its mode and source kind for offline rollback, and only current/previous build output is retained

#### Scenario: Managed field semantics
- **WHEN** a managed package record includes optional fields
- **THEN** mode is `{"pinned": REF}` or `{"branch": NAME}` (omitted for releases), catalog protects a verified catalog installation, and clone is the source-derived Git-clone key

#### Scenario: UUID identity
- **WHEN** a package is recorded in state
- **THEN** its record is keyed only by UUID without an alias hint; format remains 1 and legacy scoped records are refused without migration

