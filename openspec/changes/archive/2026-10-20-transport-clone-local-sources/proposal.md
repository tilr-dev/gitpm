# Transport clones for local sources

## Why

TILR-180: local filesystem cloning copies transient server-info files. Their removal during the copy can abort registration with ENOENT even though all committed objects remain available.

## What Changes

- Registration, catalog source discovery and installation use Git transport cloning for local paths, avoiding a raw copy of the source object directory.

## Capabilities

### Modified Capabilities
- `git-safety`: local source cloning tolerates transient server-info files.

## Impact

The three source clone calls in catalog.rs and workspace.rs; deterministic regression in dependencies.rs.
