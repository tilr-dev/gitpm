## MODIFIED Requirements

### Requirement: Lockstep crates
`gitpm` (the release-please package), `gitpm-cli` and `gitpm-ffi` SHALL share one version: release-please SHALL update the versions of `gitpm-cli` and `gitpm-ffi`, their `gitpm` dependency versions and the `Cargo.lock` entries. `gitpm-ffi` is versioned but not released (ffi-bindings, "FFI is not published").

#### Scenario: Release PR contents
- **WHEN** the release PR for 0.6.0 is opened
- **THEN** `crates/gitpm/Cargo.toml`, `crates/gitpm-cli/Cargo.toml` and `crates/gitpm-ffi/Cargo.toml` say 0.6.0
