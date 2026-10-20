## ADDED Requirements

### Requirement: The library owns domain behavior
The CLI SHALL delegate domain validation, dependency resolution and workspace mutations to the library, limiting itself to argv/environment parsing, presentation, hints and terminal prompts. Equivalent CLI and library operations SHALL produce the same package state and deployed bytes. Verified by `workspace::every_operation_rejects_invalid_ids_without_changes`.

#### Scenario: Equivalent lifecycle through CLI and library
- **WHEN** equivalent install, update, rollback, disable, enable, forget and restore operations are performed through the CLI and the public library API (the `api-parity` example)
- **THEN** after each operation the workspace state and the deployed bytes agree

#### Scenario: Domain validation is available without the CLI
- **WHEN** a caller requests a domain-invalid operation directly through the library
- **THEN** the library applies the same domain guards as a CLI caller, without relying on CLI validation: for example, every operation refuses an invalid package ID with `invalid_id` and changes nothing
