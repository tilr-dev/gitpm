## MODIFIED Requirements

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, `download`, `download-catalog`, update, `update all`, rollback, replace, `replace --back`, enable, disable, forget, restore, uninstall, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed and enabled from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`; over a disabled record from the same source it enables the package and reconciles `dist/` (install-uninstall, "Install is idempotent for the same source")

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction
