## ADDED Requirements

### Requirement: Link capability and status cross the library boundary
**BREAKING**: library and FFI SHALL expose the link operation, structured link descriptors in state/list, and link disposition with optional available version and discovery problem in check/update/update-all. Removal plans SHALL distinguish workspace entries from external targets. Link mode SHALL remain visible when availability is unknown or another problem occurs. CLI SHALL only render library results, without inferring mode or implementing version comparison or discovery policy.

#### Scenario: Host discovers a release for a link
- **WHEN** an FFI client checks or updates a linked package whose source offers a newer release
- **THEN** it receives link mode, the available version and unchanged disposition as structured data, without parsing CLI text

#### Scenario: List without release discovery
- **WHEN** a client lists packages or reads state containing a link record
- **THEN** its result identifies the external target and link mode without requiring a network request

#### Scenario: Thin CLI presentation
- **WHEN** the library reports that version 0.4.0 is available for a dev-linked package
- **THEN** the CLI explicitly reports version 0.4.0 available and that the package is dev-linked and was not updated
