## ADDED Requirements

### Requirement: The library owns domain behavior
The CLI SHALL delegate domain validation, dependency resolution and workspace mutations to the library, limiting itself to argv/environment parsing, presentation, hints and terminal prompts. Equivalent CLI and library operations SHALL produce the same package state and deployed bytes. Verified by `uc_052:lifecycle_matches_library_contract` using the `api-parity` example.

#### Scenario: Equivalent lifecycle through CLI and library
- **WHEN** equivalent install, update, rollback, enable, disable, forget, restore, fork and uninstall operations are performed through the CLI and the public library API
- **THEN** the resulting package revisions, enabled flags, fork/replacement metadata and deployed contents agree

#### Scenario: Domain validation is available without the CLI
- **WHEN** a caller requests a domain-invalid operation directly through the library
- **THEN** the library applies the same domain guards as a CLI caller, without relying on CLI validation
