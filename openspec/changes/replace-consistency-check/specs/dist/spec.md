## MODIFIED Requirements

### Requirement: Switching back redeploys the original
`replace ORIGINAL --with ORIGINAL` SHALL deploy `dist/ORIGINAL/` again from ORIGINAL's clone at its installed commit (and package folder), without a checkout of a shared clone; the former replacing package stays deployed while it is installed and enabled.

#### Scenario: Back to the original
- **WHEN** `acme/greeter` was replaced by `me/greeter` and the user runs `replace acme/greeter --with acme/greeter`
- **THEN** `dist/acme/greeter/` is deployed again and `dist/me/greeter/` remains

### Requirement: Inactive packages change state and clone only
`download`, and `install`, `update` and `rollback` of a replaced or disabled package, SHALL change state and clone only; its deployed folder SHALL appear only on `replace ORIGINAL --with ORIGINAL` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back`, `installing_a_package_that_is_replaced_deploys_nothing` and `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `acme/greeter` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/acme/greeter/` still does not exist, and a later `replace acme/greeter --with acme/greeter` deploys 0.2.0

#### Scenario: Download deploys nothing
- **WHEN** the user runs `download ./greeter`
- **THEN** `acme/greeter` is recorded disabled, its clone exists and `dist/acme/greeter/` does not

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, `download`, `download-catalog`, update, `update all`, rollback, replace, `replace ORIGINAL --with ORIGINAL`, enable, disable, forget, restore, uninstall, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed and enabled from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`; over a disabled record from the same source it enables the package and reconciles `dist/` (install-uninstall, "Install is idempotent for the same source")

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** `dist/` holds the deployed folder of a disabled package (copied into `dist/` by hand) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction
