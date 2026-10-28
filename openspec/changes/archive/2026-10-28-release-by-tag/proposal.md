## Why

Release automation must use explicit stable tags and branch-push candidates without release-please or commit-title versioning.

## What Changes

- Adopt the shared tag-based release workflow with an immutable pin.
- Set all crate and dependency versions from the selected tag in CI.
- Keep stable-only Trusted Publishing and CLI/Apple artifact uploads.
- Remove release-please configuration and generated changelog maintenance.

## Impact

- Affected specs: release-process.
- Affected code: release workflow, version preparation script and workflow integration tests.
