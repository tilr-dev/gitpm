## MODIFIED Requirements

### Requirement: Installed package record
Each `state.json` `packages.<id>` record (single source for its format) SHALL hold `source`, `current` and `previous` revisions (`version`, `commit`; `previous` also `mode`), `enabled`, `mode` (`{"pinned": REF}`, `{"branch": NAME}`, omitted for releases), and when present `forkOf`, `dependencies`, `replaces`, `catalog` (protecting verified catalog), `clone` (shared-clone key), `path` and `metadata`. At most one `previous` SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

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
