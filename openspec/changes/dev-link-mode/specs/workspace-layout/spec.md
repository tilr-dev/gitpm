## ADDED Requirements

### Requirement: State distinguishes external links from managed revisions
State SHALL support explicit link records with canonical absolute package and installation-folder paths, enabled status, a validated manifest snapshot and release-discovery source when available. Link records MUST NOT claim their contents equal an installed commit or the external repository is a managed clone. The "Installed package record" revision/clone requirements SHALL apply only to managed records.

#### Scenario: Record a live folder
- **WHEN** a local package is linked successfully
- **THEN** state explicitly records link mode and its canonical target, without an invented installed commit or managed-clone key

#### Scenario: Manifest edited outside gitpm
- **WHEN** the author edits the linked package manifest after linking
- **THEN** state keeps its last validated manifest snapshot until an explicit validated relink refreshes it

### Requirement: Link record format
A link record SHALL hold `mode` as `{"link": {"packageRoot": PATH, "target": PATH}}`, plus `source` (when available), `enabled`, `version` (declared), `dependencies` and `metadata`. It MUST NOT hold `current`, `previous`, `clone`, `path`, `catalog`, `forkOf` or `replaces`.

#### Scenario: Link record fields
- **WHEN** a package is linked
- **THEN** its record has `mode.link.packageRoot` and `mode.link.target` and no `current`, `previous` or `clone`

### Requirement: Link state has an explicit format boundary
**BREAKING**: the state format SHALL be bumped when link support is implemented, without migration. Foreign formats SHALL be refused with `unsupported` unchanged; README SHALL document workspace recreation.

#### Scenario: Old workspace reader
- **WHEN** a gitpm opens state of another format
- **THEN** it refuses with `unsupported` without rewriting it; README directs the user to recreate the workspace rather than migrate it

#### Scenario: Forgotten external link
- **WHEN** a linked package is forgotten
- **THEN** its forgotten record retains link identity and target without treating the author's repository as a retained managed clone
