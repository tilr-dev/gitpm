# Live author working trees

## Why

The research asks into live, uncommitted author changes. Dan selected
variant (b), a link to the author's working folder, in task comments #833–834
(2026-10-19). Comments #835–836 require explicit new-release reporting and
structured library/FFI results. The archived `single-ref-mode` change already
implements variant (a), following a remote branch; it explicitly excludes live
working trees. These are different operations.

## What changes

- Research deliverable: `design.md` compares (a)/(b)/(c), records the decision,
  identifies contract conflicts and proposes implementation boundaries.
- Future implementation: an explicit link operation publishes an author-owned
  installation folder as `dist/<id>` without copying, building or watching it.
- Managed install and link select the manifest `dist` folder, defaulting to the
  package root; the current managed default is `dist/` and must change.
- State distinguishes linked packages from managed clones; list, check and
  update results carry link status and release availability through library/FFI.
- Transactions manage the symlink entry, never the author's folder or contents.
- **BREAKING (future implementation):** new state representation and expanded
  library/FFI records and enums. Follow the existing release format policy
  (format 1 before first public release); no migration, aliases or compatibility
  reader. Document schema changes and foreign-format recreation in README.

## Scope and status

This task delivers research and opens this change, as its acceptance criteria
request. No link behavior has been implemented. The deltas are a future contract,
not a claim about current code. Keep this change active until implementation and
its tests land; do not archive it into `openspec/specs/` as research completion.

## Out of scope

- Implementing link in this research task (implementation and archive belong to a separate task).
- Executing build commands, running watchers and downloading builds. Reuse the
  existing manifest `dist` field for the unified publishing-folder model.
- Changing `--dev` or reintroducing channels; branch following remains available.
- Host-specific loading, hot reload, cache invalidation or lifecycle management.
- Editing, cleaning, fetching, checking out or deleting the author's working tree.

## Impact

Future changes touch workspace-layout, dist, install-uninstall, updates,
transactions-locking, ffi-bindings, package-manifest and CLI. Existing committed deployment and
lost-work rules remain in force for managed clones. See design for the precise
exceptions and the implementation/test plan.
