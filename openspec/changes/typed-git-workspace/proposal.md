# Typed Git operations for workspace management

## Why
Workspace management still builds local Git command lines outside the transport module. An embedded backend must be replaceable without changing workspace callers.

## What Changes
- Add typed transport operations for workspace object queries, refs, checkout, configuration and fork creation.
- Migrate production workspace, clone and fork callers while preserving existing behavior and error handling.
- Keep test fixture setup outside this migration.

## Impact
- Affected specification: git-safety.
- Affected code: workspace management and the internal transport module.
