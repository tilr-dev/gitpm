# Typed scaffold Git operations

## Why
Authoring still assembles Git commands and exposes raw transport helpers. A replacement backend must cover authoring and be selected in one place.

## What Changes
- Route scaffold authoring through typed transport operations while preserving repository mutations, errors, SSH settings, identity and rollback behavior.
- Remove the raw Git API visible outside the transport and select the backend through one internal factory used by all transport facades.
- Keep test fixtures, integration tests and build scripts outside the production boundary.

## Impact
- git-safety transport requirements and the production transport boundary check.
- Internal library implementation only; public commands and behavior remain unchanged.
