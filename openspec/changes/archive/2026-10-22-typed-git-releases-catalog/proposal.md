# Typed Git operations for releases and catalog

## Why

The transport boundary already exposes typed network operations, while release,
catalog and repository inspection code still assembles local Git commands. Moving
those commands behind typed operations keeps a future embedded backend confined
to the transport module.

## What changes

- Add typed local operations for release discovery, lost-work inspection, catalogs, checks,
  model resolution, package discovery and authentication support.
- Preserve existing results, error kinds, SSH policy and subprocess bounds.
- Enforce the migrated boundary with a production-source check and exercise the
  operations against local Git repositories.

## Non-goals

- Embedded Git or HTTP implementations.
- Migration of workspace, clone and fork callers or test fixture commands.
- CLI, public API, file-format or host-specific changes.
