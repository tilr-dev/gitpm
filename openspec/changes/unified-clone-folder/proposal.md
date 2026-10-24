## Why

Git clones currently use different workspace folders depending on whether the package lives at the repository root. A single source-keyed clone layout makes ownership and cleanup consistent across root and nested packages.

## What Changes

- Store all managed Git clones under `repos/<name-hash>`, with explicit clone keys and optional package paths in state.
- Stop creating or consulting workspace `packages/`; do not migrate or remove existing user clones. Keep state format 1.
- Preserve shared clone ownership, local work guards, transactional lifecycle operations and clone-free catalog builds.

## Impact

Workspace layout, install and removal, updates and rollback, fork and replacement, forgotten records, dist cleanup, CLI/FFI examples and integration coverage.
