# Skip unnecessary bare repository staging

## Why
Removing a package worktree copies the entire bare repository even when its registration is absent or the bare repository will be deleted. This wastes disk space and rewrites untouched repositories.

## What changes
- Check for a live worktree registration before staging its bare repository.
- Schedule unused bare repositories for deletion before removing their worktrees.
- Cover last-user removal and build updates sharing a retained source store.

## Impact
Transaction staging and package removal; workspace format and public APIs stay unchanged.
