# UUID package identity

## Identity and names

The manifest carries a canonical lowercase hyphenated UUID `id` and a required `name` consisting of 1 to 80 ASCII letters, digits, `-` or `_`, without an initial `-`. `init NAME` generates an ID once; commit preserves it. Names are metadata and may repeat. Package references carry UUIDs alone; commands do not resolve names.

## Storage and interfaces

State keys, dependency keys, provenance and replacement records use UUIDs. Deployments live at `dist/<uuid>`, catalog indexes at `packages/<uuid>/plugin.json`, and keep-alive refs at `refs/gitpm/<uuid>/{current,previous}`. New package folders selected by authoring use the generated UUID beneath the repository package pattern, allowing duplicate names. Existing generic `<id>` placeholders mean UUID. Git clones remain source-keyed because one repository can contain multiple packages. Workspace and catalog format numbers remain unchanged; old scoped IDs are rejected without conversion.

## Fork and replacement

Fork clones the full recorded source repository, generates a fresh UUID per package and preserves its name. The original UUID is recorded in `forkOf.id` and `replaces.id`. A complete original-to-fork map rewrites dependencies between repository packages to their new UUIDs using the existing range behavior. `--ns` is removed. Global replacement maps an original UUID to a provider UUID; consumer manifests and dependency keys remain unchanged.

## Validation

Integration coverage exercises generated and persistent IDs, equal-name coexistence, scoped-ID rejection, UUID paths, fresh fork identities and metadata, internal fork dependency mapping, and replacement without consumer reference rewrites. Existing operation and FFI coverage is adapted to UUID fixtures.
