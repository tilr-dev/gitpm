# Design

## Storage and identity

`repos/<source-key>` is a bare repository with remote-tracking origin branches and
mirrored tags. It has no manager-created local branches. Its symbolic HEAD follows
the origin's default branch for discovery only; package guards never consult it.
`worktrees/<namespace>/<name>` is a detached linked worktree, with the full source
tree at the package's recorded commit. `Installed.path` selects the package folder
inside that tree. The worktree's Git administration directory is keyed by SHA256 of
the package ID, avoiding collisions between equal package names in different
namespaces. No additional state field is needed.

## Transactions and recovery

A transaction stages the bare store once and stages only affected package worktrees.
An existing worktree copy keeps its index and files, and both Git links are rebound
to staged paths before checkout. New worktrees use the typed transport operation.
The transaction journal includes checkout-to-administration mappings. After path
renames, commit and recovery bind both `.git` and `gitdir` to live paths, with
`commondir` relative to the administration directory. These writes and their parents
are synced before state publication. Recovery can repeat this phase after either
side was written. Siblings keep their own administration HEAD and index; replacing
the common bare store repairs their links without checking them out.

Git reports canonical administration paths, so validation compares canonical parent
paths even when the workspace was opened through an ancestor alias.

## Lifecycle

Git installs, updates and rollbacks create or move only the target worktree. All
Git modes require its recorded HEAD and clean files; uninstall retains its existing
explicit lost-work confirmation mechanism. Forget keeps its worktree unchanged;
restore uses that recorded revision and never adopts a different branch HEAD.
A build has no worktree. A previous Git revision retains bare objects and keep-alive
refs for offline rollback; once both retained revisions are builds, an otherwise
unused bare store is released. Uninstall removes the target administration entry
and checkout, then removes the bare store only with its last user. Hidden repository
work is inspected before deleting the common store.

## Compatibility

The pre-release state format remains 1. There is no migration or persistent support
for checked-out source clones from the previous layout. Opening a workspace does
not remove those directories automatically.
