## ADDED Requirements

### Requirement: Linked packages report releases without automatic replacement
Single-package update, update all and check SHALL retain link records and report link mode explicitly. They SHALL discover releases under existing source/selection policies without modifying the author repository and report a newer version with dev-linked, unchanged status. An explicit target MUST NOT silently convert a link into a managed installation.

#### Scenario: A new release is available
- **WHEN** a dev-linked package declares version 0.3.0 and its release source offers 0.4.0
- **THEN** check, update and update all report version 0.4.0 available, explicit dev-link mode and no package update, leaving the target and its files unchanged

#### Scenario: Explicit update target preserves a link
- **WHEN** update of a linked package requests `--ref`, `--dev` or `--release`
- **THEN** its structured result reports dev-linked and unchanged, preserving its descriptor, symlink and author files rather than switching to managed installation

#### Scenario: No newer release
- **WHEN** discovery finds no newer release for a linked package
- **THEN** results still identify it as dev-linked and unchanged

### Requirement: Link discovery failures stay explicit
When release discovery fails, results SHALL preserve link status and describe the availability problem without claiming the package is up to date.

#### Scenario: Release source unavailable
- **WHEN** a linked package's release source cannot be read
- **THEN** its result retains link mode and an explicit discovery problem rather than reporting up to date

### Requirement: Joint updates keep link constraints and visibility
The joint update solver SHALL keep each linked package's recorded version and dependencies as fixed constraints, never replace its contents or install dependencies on its behalf. Link disposition SHALL remain visible alongside dependency or availability problems. Dry-run results SHALL carry the same link disposition and release availability without writes. Healthy managed packages SHALL continue to update according to their own constraints and modes.

#### Scenario: Mixed managed and linked packages
- **WHEN** update all considers one linked package and a managed package with a compatible newer release
- **THEN** the linked package is explicitly reported unchanged and dev-linked, while the managed package can update

#### Scenario: Link constrains a managed dependency
- **WHEN** a linked package's recorded dependency excludes a newer managed dependency version
- **THEN** the joint solution respects that constraint without changing the author's files

#### Scenario: Preview
- **WHEN** update all runs in dry-run mode with a linked package
- **THEN** it reports the same dev-link status and discovered release information without modifying state or deployed entries

## MODIFIED Requirements

### Requirement: update follows the recorded mode
Managed `update ID` SHALL follow recorded mode: release moves to the newest release, branch to HEAD, pinned stays with reason `pinned` (CLI: `ID pinned at V (REF)`). Explicit targets SHALL switch managed mode as at install. Links follow "Linked packages report releases without automatic replacement", including explicit targets. Verified by `workspace::update_follows_and_switches_the_recorded_mode`.

#### Scenario: Pinned package
- **WHEN** `acme/greeter` was installed with `--ref acme/greeter@0.1.0`, 0.2.0 is released and the user runs `update acme/greeter`
- **THEN** it prints `acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)` and nothing changes

#### Scenario: Back to releases
- **WHEN** the user then runs `update acme/greeter --release`
- **THEN** it prints `acme/greeter updated 0.1.0 → 0.2.0 (major)` and the record has no `mode`

#### Scenario: Explicit managed targets
- **WHEN** a managed package is updated with `--ref REF`, `--dev` or `--release`
- **THEN** the target and recorded mode follow install semantics: explicit ref, origin default branch or newest release respectively
