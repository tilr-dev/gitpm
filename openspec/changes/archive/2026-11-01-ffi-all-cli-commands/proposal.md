# Expose every CLI command through the native library

## Why
Native hosts embed gitpm and need author operations and catalog writes without installing the CLI. Existing FFI coverage excludes these operations.

## What Changes
- Export initialization, release commits, remote configuration and catalog writing through UniFFI using the existing library implementation.
- Remove command coverage exceptions and verify the new records and operations.
- Generate matching Swift bindings through the existing Apple artifact build.

## Impact
- ffi-bindings: manager surface and complete CLI parity.
- gitpm-ffi: additive native exports and result records.
- Existing CLI behavior and release workflow remain the same.
