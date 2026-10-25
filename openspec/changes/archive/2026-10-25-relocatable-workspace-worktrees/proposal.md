## Why

Copying or moving a workspace leaves absolute Git worktree links pointing at its old location. Installed and forgotten Git packages must remain usable at the new root without manual repair or a newer Git requirement.

## What Changes

- Describe relocation repair as workspace-open maintenance under the workspace lock, separate from transactions.
- Derive checkout/admin pairs from validated state and repair stale links after journal recovery, without consulting the old root.
- Remove the integration-test workaround and cover copied and moved workspaces.

## Impact

- Adds a workspace-layout requirement; transaction staging and read-only views retain their existing behavior.
- Repairs only managed Git metadata; package contents, commits, state and the source workspace are preserved.
