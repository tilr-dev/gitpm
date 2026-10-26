## Why

Integration scenarios still assume checked-out source clones and older diagnostics.
The suite must exercise the documented bare repository and package worktree layout.
Layout changes also need an explicit integration gate because the default checks do
not run the Dagger suite.

## What Changes

- Align fixture invariants, failure injection and assertions with current contracts.
- Document why missing source storage preserves orphan worktrees.
- Require the full Dagger suite before review and merge of workspace layout changes.

## Impact

- Affected specification: integration-tests (one added requirement).
- Affected code: integration scenarios and helpers; no product behavior changes.
