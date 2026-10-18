## MODIFIED Requirements

### Requirement: Inactive packages change state and clone only
`download`, and `install`, `update` and `rollback` of a replaced or disabled package, SHALL change state and clone only; the deployed folder SHALL appear only on `replace --back` or `enable`. Verified by `update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back`, `installing_a_package_that_is_replaced_deploys_nothing` and `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Update of a replaced package
- **WHEN** a replaced `acme/greeter` is updated from 0.1.0 to 0.2.0
- **THEN** its state shows 0.2.0, `dist/acme/greeter/` still does not exist, and a later `replace acme/greeter --back` deploys 0.2.0

#### Scenario: Download deploys nothing
- **WHEN** the user runs `download ./greeter`
- **THEN** `acme/greeter` is recorded disabled, its clone exists and `dist/acme/greeter/` does not

### Requirement: Enable and disable deploy and remove
`disable ID` SHALL remove `dist/ID/` and `enable ID` SHALL deploy it again from the clone. The `has_dependents` guard on disabling a replacing package or a dependency, and the dependency check of `enable`, are specified in dependencies ("Dependents guard removal", "enable needs active dependencies"); this requirement covers only their effect on `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable acme/greeter` and then `enable acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `me/greeter` replaces `acme/greeter` and the user runs `disable me/greeter --force`
- **THEN** `dist/me/greeter/` is removed and, because `acme/greeter` stays replaced, `dist/acme/greeter/` is not deployed

#### Scenario: Refused enable
- **WHEN** `enable app` is refused because `lib` is disabled
- **THEN** `dist/app/` is not created

### Requirement: Every mutating command reconciles all of dist
Every mutating command (install, `install-catalog`, `download`, `download-catalog`, update, `update all`, rollback, replace, `replace --back`, enable, disable, forget, restore, uninstall, `fork --replace`, `sync-dist`) SHALL, inside its transaction, deploy the missing deployed folders of active packages and remove those of inactive known packages. An existing deployed folder of an active package MUST NOT be rewritten. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Repeated install
- **WHEN** `install` finds the package already installed from the same source
- **THEN** it changes nothing, is not a mutating command and does not reconcile `dist/`

#### Scenario: Stale folder from gitpm 0.4
- **WHEN** a workspace holds a deployed folder of a disabled package (as gitpm 0.4 left them) and the user installs an unrelated package
- **THEN** the disabled package's `dist/` is removed in the same transaction
