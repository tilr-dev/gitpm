## ADDED Requirements

### Requirement: State distinguishes external links from managed revisions
State SHALL support explicit link records with canonical absolute package and installation-folder paths, enabled status, a validated manifest snapshot and release-discovery source when available. Link records MUST NOT claim their contents equal an installed commit or the external repository is a managed clone.

#### Scenario: Record a live folder
- **WHEN** a local package is linked successfully
- **THEN** state explicitly records link mode and its canonical target, without an invented installed commit or managed-clone key

#### Scenario: Manifest edited outside gitpm
- **WHEN** the author edits the linked package manifest after linking
- **THEN** state keeps its last validated manifest snapshot until an explicit validated relink refreshes it

### Requirement: Link record format
A link record SHALL hold `mode` as `{"link": {"packageRoot": PATH, "target": PATH}}`, optional validated `mode.link.discovery` provenance, plus `source` (when available), `enabled`, `version` (declared), `dependencies` and `metadata`. It MUST NOT hold `current`, `previous`, `clone`, `path`, `catalog`, `forkOf` or `replaces`.

#### Scenario: Link record fields
- **WHEN** a package is linked
- **THEN** its record has `mode.link.packageRoot` and `mode.link.target` and no `current`, `previous` or `clone`

### Requirement: Linked fork discovery preserves validated provenance
A fork link SHALL record its validated `forkOf` snapshot as `mode.link.discovery.forkOf`, with the same field shapes and source validation as package-manifest. Check and update SHALL use this snapshot for original-upstream discovery until validated relink refreshes it, without rereading external manifest edits into state.

#### Scenario: Fork manifest changed outside gitpm
- **WHEN** the author changes or removes `forkOf` in the linked manifest after linking
- **THEN** check still reports original-upstream availability from the recorded provenance; a validated relink may refresh it

#### Scenario: Link a newly prepared fork
- **WHEN** a repository prepared by fork is explicitly installed in link mode
- **THEN** the selected package's validated manifest `forkOf` is recorded in `mode.link.discovery.forkOf`, without a top-level `forkOf` or `replaces` and without creating a replacement binding

#### Scenario: Refresh fork provenance explicitly
- **WHEN** the author explicitly relinks a package after changing its manifest `forkOf` to another valid provenance descriptor
- **THEN** successful validation refreshes the recorded discovery snapshot and subsequent check and update use the refreshed original-upstream provenance

### Requirement: Link records follow the workspace format policy
Link record schema changes SHALL follow workspace-layout "State format versions": format 1 in place before the first public release; later bumps only under its release-milestone policy. No migration or compatibility reader SHALL be added. Foreign formats SHALL be refused with `unsupported` unchanged; README SHALL document workspace recreation.

#### Scenario: Old workspace reader
- **WHEN** a gitpm opens state of another format
- **THEN** it refuses with `unsupported` without rewriting it; README directs the user to recreate the workspace rather than migrate it

#### Scenario: Forgotten external link
- **WHEN** a linked package is forgotten
- **THEN** its forgotten record retains link identity and target without treating the author's repository as a retained managed clone

## MODIFIED Requirements

### Requirement: Installed package record
Each `state.json` `packages.<id>` managed record SHALL hold `source`, `current`, at most one `previous`, `enabled`, and optional `mode`, `forkOf`, `dependencies`, `replaces`, `catalog`, `clone`, `path`, `metadata`. Revisions contain `version` and `commit`; `previous` also retains mode. Links follow "Link record format". Verified by `model::state_round_trips_byte_for_byte`.

#### Scenario: Fresh install has no previous revision
- **WHEN** a package was just installed
- **THEN** its record has no `previous` revision

#### Scenario: Update keeps one previous revision
- **WHEN** a package goes from 0.1.0 to 0.2.0 to 0.3.0
- **THEN** `current` is 0.3.0 and `previous` is 0.2.0 only

#### Scenario: Recorded modes
- **WHEN** packages were installed without a ref, with `--ref acme/a@1.0.0`, and with `--ref feature`
- **THEN** the first record has no `mode`, the second `"mode": {"pinned": "acme/a@1.0.0"}`, the third `"mode": {"branch": "feature"}`

#### Scenario: Previous revision keeps its mode
- **WHEN** a package in branch mode `main` is updated with `--release`
- **THEN** `previous` is `{"version": …, "commit": …, "mode": {"branch": "main"}}` (a `previous` in release mode has no `mode`), so `rollback` restores both

#### Scenario: Managed field semantics
- **WHEN** a managed package record includes optional fields
- **THEN** mode is `{"pinned": REF}` or `{"branch": NAME}` (omitted for releases), catalog protects a verified catalog installation, and clone is the shared-clone key
