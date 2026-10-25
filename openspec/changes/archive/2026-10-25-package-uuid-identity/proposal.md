# UUID package identity

## Why

Package identity currently combines a namespace with a name. Renaming or forking changes that identity and prevents stable references to an original package when a fork provides it. UUIDs make identity independent of names and ownership.

## What Changes

- Generate an immutable UUID in `init NAME` and retain name as a nonunique manifest alias.
- Require UUIDs in all package references, CLI selectors and FFI operations; remove namespace handling and `fork --ns`.
- Use UUID deployment paths, catalog package paths and keep-alive refs.
- Generate fresh UUIDs for every forked package, preserving names and UUID provenance; retain internal dependency rewriting to the new fork UUIDs.
- Preserve explicit replacement bindings without rewriting consumer references.
- Keep format 1, reject legacy identities and provide no migration.

## Impact

Breaking changes to manifests, workspace data, catalog layout, authoring and fork APIs. Source-keyed Git clones continue to be shared by packages from one source.
