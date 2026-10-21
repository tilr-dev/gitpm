# Fork source repositories with Git

## Why

Copying an installed worktree confuses installation state with package sources and loses submodules when exporting package folders.

## What Changes

- Clone the recorded source with history at the installed revisions into a new folder.
- Preserve submodule gitlinks and configuration for initialization with Git.
- Ignore local edits and commits of the installed clone.
- Remove adding packages to an existing fork; rewrite dependencies only among packages forked together.
- Keep installation from committed dist unchanged.

## Impact

- Affected specs: fork-replacement, multi-package-repos
- Affected code: workspace fork operations, CLI help, README and integration tests
