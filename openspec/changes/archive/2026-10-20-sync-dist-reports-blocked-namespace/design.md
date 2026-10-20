# Blocked namespace reporting

`dist_drift` already records a file or symlink at `dist/<ns>` as unmanaged, but its
active-package loop continued past packages below that path without recording them.
Consequently `sync_dist` received an empty `skipped` list and returned success even
though those packages had no deployed `dist/<id>`. The extended
`a_namespace_path_that_is_not_a_folder_blocks_nothing_else` test reproduced that
success before the fix.

Keep the blocked path in `unmanaged`, and add each active package below it to
`skipped` with `dist/<ns> is not a folder`. Existing `sync_dist` handling then reports
`not_deployed` while reconciling unrelated packages. Transactions that attempt to
delete a managed path below the file or symlink still fail with `workspace`.

The review regression test also reproduced misleading CLI advice: `hint()` handled
all `NotDeployed` errors as missing clones, even for blocked namespaces. Select the
hint from the first skipped item's reason: retain uninstall/forget only when
`clone_missing` is true, suggest removing or renaming the namespace path for the
blocked-path reason, and omit a hint for other deploy failures. This keeps the
existing public report format unchanged. The file and symlink test asserts the
correct hint and then removes the obstruction and successfully redeploys the package.
