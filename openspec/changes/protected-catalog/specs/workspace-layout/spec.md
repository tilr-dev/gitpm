## MODIFIED Requirements

### Requirement: Installed package record
Single source for the record format: each `state.json` `packages.<id>` record SHALL hold `source`, `current`/`previous` revisions (`version`, `commit`), `enabled`, `mode` (omitted in release mode, else `{"pinned": REF}` or `{"branch": NAME}`), and when present `forkOf`, `dependencies`, `replaces`, `catalog` (protecting verified catalog), `clone` (shared-clone key), `path` and `metadata`. At most one previous revision SHALL be kept. Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

#### Scenario: Recorded modes
- **WHEN** packages were installed without a ref, with `--ref acme/a@1.0.0`, and with `--ref feature`
- **THEN** the first record has no `mode`, the second `"mode": {"pinned": "acme/a@1.0.0"}`, the third `"mode": {"branch": "feature"}`

