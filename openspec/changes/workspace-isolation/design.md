# Workspace selection audit

## Public entry points

`Manager::open(root)` and `open_with(root, options)` require a root and store its absolute path. All install, download, update, rollback, enable/disable, forget/restore, uninstall, replacement, fork and dist synchronization methods operate through that Manager. State reads use its root; clones, builds and transactions are rooted there too. The process-wide held-lock set is keyed by the canonical lock path, allowing roots with the same basename under different parents.

The free library functions `check::read_state`, `check::check` and `list_with` take a root directly. Catalog functions take an explicit catalog location; authoring, package inspection, release and authentication functions take an explicit repository path or source and do not select an implicit workspace.

The FFI `PackageManager::open(root, options)` wraps the Manager. Its workspace methods use that handle; free `list`, `read_state` and `check` receive a root. No separate FFI workspace state is stored globally.

The CLI resolves `--root` before `GITPM_ROOT` and the documented default, then passes it to Manager or the free read APIs. Repository-only init/commit/remote/auth and catalog commands use their explicit destinations or catalog locations without opening a Manager. Fork reads the selected workspace and writes only to its explicit destination.

No change to runtime workspace selection is needed for these entry points.

## Verification design

The library integration test keeps two Managers open with roots named `workspace` in different parents, installs the same package in both, removes and reinstalls in either direction, and compares recursive byte snapshots of the untouched root, including Git internals and empty directories. Reopening verifies independent persisted state.

The CLI integration test invokes every existing command with an explicit root or repository destination. Both its working directory and `GITPM_ROOT` select the other workspace. Each call compares that workspace's recursive snapshot; lifecycle assertions check deployed output and state in the selected root. Repository-only calls additionally use a missing root and verify that neither it nor its lock is created. All sources and catalogs are local fixtures.
