# Live author working trees

## Why

The research asks into live, uncommitted author changes. Dan selected
variant (b), a link to the author's working folder (owner decisions of
2026-10-19). Those decisions also require explicit new-release reporting and
structured library/FFI results. The archived `single-ref-mode` change already
implements variant (a), following a remote branch; it explicitly excludes live
working trees. These are different operations.

## What changes

- `design.md` compares (a)/(b)/(c), records the decision and defines the
  implementation boundaries.
- An explicit link operation publishes an author-owned
  installation folder as `dist/<id>` without copying, building or watching it.
- Managed install and link select the manifest `dist` folder, defaulting to the
  existing `"dist"` folder; explicit `"."` selects the package root.
- State distinguishes linked packages from managed clones; list, check and
  update results carry link status and release availability through library/FFI.
- Transactions manage the symlink entry, never the author's folder or contents.
- **BREAKING:** new state representation and expanded
  library/FFI records and enums. Follow the existing release format policy
  (format 1 before first public release); no migration, aliases or compatibility
  reader. Document schema changes and foreign-format recreation in README.

## Scope

The explicit local link operation spans the library, state, deployment lifecycle,
release reporting, FFI and CLI. Research established the contract; implementation
and hermetic integration scenarios complete this change.

## Out of scope

- Executing build commands, running watchers and downloading builds. Reuse the
  existing manifest `dist` field for the unified publishing-folder model.
- Changing `--dev` or reintroducing channels; branch following remains available.
- Host-specific loading, hot reload, cache invalidation or lifecycle management.
- Editing, cleaning, fetching, checking out or deleting the author's working tree.

## Impact

Changes touch workspace-layout, dist, install-uninstall, updates,
transactions-locking, ffi-bindings and CLI. Existing committed deployment and
lost-work rules remain in force for managed clones. See design for the precise
exceptions and the implementation/test plan.
