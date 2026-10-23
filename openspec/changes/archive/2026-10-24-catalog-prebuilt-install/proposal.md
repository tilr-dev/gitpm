## Why
Catalog releases already describe prebuilt output, but installations still need the source repository. Published builds should install and update without a clone and retain an offline rollback target.

## What Changes
- Download catalog builds, verify SHA-256 and release file hashes, and unpack ready output in process.
- Retain current and previous verified output locally through workspace transactions.
- Use catalog metadata for build installs; use Git for releases without builds and explicit ref/dev/detach operations.
- Expose the behavior through the existing library, CLI and FFI entrypoints.

## Capabilities
### Modified Capabilities
- catalog
- install-uninstall
- dist
- updates
- workspace-layout
- git-safety

## Impact
Library state and transport, workspace lifecycle, archive dependencies, integration tests and product documentation.
