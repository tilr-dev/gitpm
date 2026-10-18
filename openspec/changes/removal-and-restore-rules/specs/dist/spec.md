## MODIFIED Requirements

### Requirement: Enable and disable deploy and remove
`disable ID` SHALL remove `dist/ID/` (for an active replacing package also drop the binding and deploy ORIGINAL when active); `enable ID` SHALL deploy it again from the clone. The `has_dependents` guard on disabling a replacing package or a dependency, and the dependency check of `enable`, are specified in dependencies ("Dependents guard removal", "enable needs active dependencies"); this covers only `dist/`. Verified by `disable_removes_and_enable_restores_also_around_a_replacement`.

#### Scenario: Disable and enable
- **WHEN** the user runs `disable acme/greeter` and then `enable acme/greeter`
- **THEN** `dist/acme/greeter/` is removed by the first and deployed again from the clone's installed commit by the second

#### Scenario: Forced disable of an active replacement
- **WHEN** `me/greeter` replaces `acme/greeter`, which is not installed, `app` depends on `acme/greeter`, and the user runs `disable me/greeter --force`
- **THEN** `dist/me/greeter/` is removed, the binding is dropped and nothing is deployed for `acme/greeter`; `app` is reported broken

#### Scenario: Refused enable
- **WHEN** `enable app` is refused because `lib` is disabled
- **THEN** `dist/app/` is not created

### Requirement: Undeployable packages do not block others
An active package whose clone or installed commit is missing SHALL be skipped by reconciliation so it does not block an unrelated command; the CLI SHALL warn `Warning: ID: dist not deployed (clone missing); uninstall or forget ID to drop its record`. Any other deploy error SHALL fail the command. Verified by `a_missing_clone_is_reported_and_blocks_only_its_own_dist`.

#### Scenario: Clone deleted by hand
- **WHEN** the clone of an active package was deleted and the user enables another package
- **THEN** the enable succeeds and a warning names the package whose dist was not deployed and the way out (install-uninstall, "Removing a package whose clone is gone")

