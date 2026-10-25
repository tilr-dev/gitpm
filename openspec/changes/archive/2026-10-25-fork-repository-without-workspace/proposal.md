# Fork a repository without installation

## Why

Fork currently requires installed IDs and a workspace. Authors need to fork a source repository directly, preserving its history, files and all packages.

## What Changes

- Add `gitpm fork ID [ID...] --from SOURCE --ns NAMESPACE --into DIR [--ref REF]`.
- Clone the whole repository; rename every package and rewrite internal dependencies using shared fork logic.
- Without `--ref`, choose the latest stable repository release containing the first ID; validate every supplied ID at that revision; with `--ref`, accept any revision.
- Expose a free library function and FFI `forkSource`, without opening, creating or locking a workspace.

## Capabilities

### Modified Capabilities
- `fork-replacement`: source selection, metadata and isolation.
- `multi-package-repos`: source repository selection with all packages retained.
- `cli`: source and revision option validation.

## Out of Scope

Forking uncommitted edits or installing/replacing the fork in one step.
