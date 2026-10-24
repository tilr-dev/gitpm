# Explicit workspace selection and isolation

## Why

A host can keep its active workspace while preparing a new configuration in another directory. Every workspace operation must target the selected directory and preserve all other workspaces.

## What Changes

- Add a workspace-layout requirement for explicit workspace selection and independent state and files.
- Audit existing library, FFI and CLI entry points and verify them with integration tests using separate workspace roots.
- Preserve the CLI's documented default root and repository-only commands' explicit repository paths.

## Impact

- Affected specification: workspace-layout.
- Existing workspace operations and integration tests; no new commands or workspace relocation behavior.
