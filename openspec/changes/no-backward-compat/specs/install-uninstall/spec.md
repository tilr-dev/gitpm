## MODIFIED Requirements

### Requirement: Same ID from another source is refused
Installing an ID that is installed from a different source (including a `file://` URL against a plain path, or a URL with and without `.git`) SHALL fail with `already_installed`. Installing an ID that is forgotten but retained SHALL be refused with `already_installed` until it is restored or uninstalled. Verified by `forgotten_repository_blocks_reinstall_until_restored`.

#### Scenario: Other source
- **WHEN** `acme/greeter` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

#### Scenario: Forgotten but retained
- **WHEN** `acme/greeter` was forgotten and its clone `packages/acme/greeter` is retained, and the user installs it again
- **THEN** the command fails with `already_installed` ("repository already exists: acme/greeter; use restore") and the workspace is unchanged
- **AND** for a forgotten package of a shared clone the message is `ID was forgotten and its files are kept; use restore, or uninstall it first`

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (keep everything, set `enabled: false`, remove `dist/<id>`), `forget` (drop the package from `packages` in state, keep the clone, remove `dist/<id>`) and `uninstall` (remove the state entry, the clone and `dist/<id>`). `restore ID` SHALL record a retained clone in state again. All three removals are guarded as in dependencies ("Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Uninstall one package of a shared clone
- **WHEN** another package still uses the shared clone of the package being uninstalled
- **THEN** the clone stays; it is removed only with its last package (multi-package-repos, "One shared clone per source")

#### Scenario: Forget then restore
- **WHEN** the user runs `forget acme/greeter` then `restore acme/greeter`
- **THEN** the first prints `acme/greeter forgotten (clone kept; restore re-registers it)`, the second `acme/greeter restored 0.2.0`, and `dist/acme/greeter/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID whose own clone does not exist
- **THEN** the command fails with `repository_not_found` (a forgotten package whose shared clone is gone fails with `workspace`, see multi-package-repos)

### Requirement: Restore preconditions
`restore ID` of an own clone (`packages/<id>`) SHALL refuse with `already_installed` ("package ID is already registered") when the ID is still recorded in state, with `local_changes` when the clone has modified, untracked or ignored files, and with `invalid_manifest` ("package id mismatch") when the manifest at HEAD has another ID. It SHALL print `ID restored V`. Shared-clone restore: see multi-package-repos.

#### Scenario: Restored record
- **WHEN** a clean retained own clone is restored
- **THEN** state records its HEAD as `current` (whatever commit it is) with no `previous`, `source` taken from the clone's `origin` URL (`git` when it has none), no `catalog` and `enabled: true`

#### Scenario: Dirty retained clone
- **WHEN** `acme/greeter` was forgotten, a file in `packages/acme/greeter` was edited and the user runs `restore acme/greeter`
- **THEN** the command fails with `local_changes` and the package stays out of state
