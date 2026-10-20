## Why

TILR-168: Dagger runs can execute a CLI older than their source tree. The runner shares a mutable Cargo target directory across different worktrees and copies executables from it after Cargo releases its build lock.

## What Changes

- Bind target caches to the complete build source snapshot, separately for test builds and clippy.
- Keep reusable compilation outputs for identical snapshots and the shared Cargo download cache.
- Add a controlled Dagger regression with distinguishable Cargo workspaces, including concurrent builds.

## Capabilities

### Modified Capabilities
- `integration-tests`: require source ownership of cached executables.

## Impact

Only the integration runner and its build cache policy change. CLI and library behavior remain as specified.
