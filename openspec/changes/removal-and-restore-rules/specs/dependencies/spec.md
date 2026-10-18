## MODIFIED Requirements

### Requirement: Dependents guard removal
Single source for the `has_dependents` guard: `disable`, `forget` and `uninstall` SHALL refuse with `has_dependents` unless forced exactly when a consumer's dependency would break: one that resolves to the package, or, for an active replacing package, one ORIGINAL does not satisfy once the binding is dropped. `dependents` and `UninstallPlan.dependents` SHALL name exactly those consumers. With force each SHALL be allowed. Verified by `dependents_guard_disable_forget_and_uninstall`.

#### Scenario: Uninstall a replacing package with consumers
- **WHEN** `app` depends on `acme/greeter`, which `me/greeter` replaces, `acme/greeter` is not installed, and the user runs `uninstall me/greeter`
- **THEN** it fails with `has_dependents` naming `app`

#### Scenario: Forget a replacing package without consumers
- **WHEN** `me/greeter` replaces `acme/greeter`, nothing depends on `acme/greeter`, and the user runs `forget me/greeter`
- **THEN** it succeeds without force and drops the binding (fork-replacement, "Active fork rules")

#### Scenario: Disable an active replacement without force
- **WHEN** `me/greeter` replaces `acme/greeter`, `app` needs `acme/greeter ^2`, the installed `acme/greeter` is 1.4.0, and the user runs `disable me/greeter`
- **THEN** the command fails with `has_dependents` naming only `app` and both deployed folders are unchanged
- **AND** without such a consumer the disable succeeds, drops the binding and deploys `acme/greeter` again; this is covered by `disable_removes_and_enable_restores_also_around_a_replacement`

#### Scenario: Uninstall a dependency
- **WHEN** `app` depends on `lib` and the user runs `gitpm uninstall lib`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` lib is removed

#### Scenario: Installed original satisfies the consumers
- **WHEN** `me/greeter` replaces `acme/greeter`, `app` needs `acme/greeter ^1` and the installed, enabled `acme/greeter` is 1.4.0
- **THEN** `uninstall me/greeter`, `forget me/greeter` and `disable me/greeter` succeed without force and `app` resolves to `acme/greeter` again
