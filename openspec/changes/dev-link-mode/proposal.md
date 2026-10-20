# Live author working trees

## Why

TILR-35 asks for research into live, uncommitted author changes. Dan selected
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
- State distinguishes linked packages from managed clones; list, check and
  update results carry link status and release availability through library/FFI.
- Transactions manage the symlink entry, never the author's folder or contents.
- **BREAKING (future implementation):** new state representation/format and
  expanded library/FFI records and enums. Re-create workspaces for the new format;
  no migration, aliases or compatibility reader. Document in README when shipped.

## Scope and status

TILR-35 delivers research and opens this change, as its acceptance criteria
request. No link behavior has been implemented. The deltas are a future contract,
not a claim about current code. Keep this change active until implementation and
its tests land; do not archive it into `openspec/specs/` as research completion.

## Out of scope

- Implementing link in this research task.
- Build commands, watchers, downloadable builds and the manifest installation
  folder schema (TILR-142). This change consumes that schema once defined.
- Changing `--dev` or reintroducing channels; branch following remains available.
- Host-specific loading, hot reload, cache invalidation or lifecycle management.
- Editing, cleaning, fetching, checking out or deleting the author's working tree.

## Impact

Future changes touch workspace-layout, dist, install-uninstall, updates,
transactions-locking, ffi-bindings and CLI. Existing committed deployment and
lost-work rules remain in force for managed clones. See design for the precise
exceptions and the implementation/test plan.
