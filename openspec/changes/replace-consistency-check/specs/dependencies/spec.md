## MODIFIED Requirements

### Requirement: Dependents guard removal
Single source for `has_dependents`: `disable`, `forget` and `uninstall` SHALL refuse with it unless forced exactly when a consumer's dependency would break: one resolving to the package directly, or through a binding ORIGINAL does not take over (installed, enabled, matching version). `dependents` and `UninstallPlan.dependents` SHALL name exactly those consumers. Verified by `dependents_guard_disable_forget_and_uninstall` and `removing_an_active_fork_guards_only_consumers_the_original_breaks`.

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

#### Scenario: Installed original satisfies the consumers
- **WHEN** `me/greeter` replaces `acme/greeter`, `app` needs `acme/greeter ^1` and the installed, enabled `acme/greeter` is 1.4.0
- **THEN** `uninstall me/greeter`, `forget me/greeter` and `disable me/greeter` succeed without force and `app` resolves to `acme/greeter` again

#### Scenario: Uninstall a dependency
- **WHEN** `app` depends on `lib` and the user runs `gitpm uninstall lib`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` lib is removed

#### Scenario: Forced removal
- **WHEN** the user passes --force to disable, forget or uninstall
- **THEN** removal is allowed and broken consumers are reported; returning through replace ORIGINAL --with ORIGINAL has no force bypass

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every direct dependency no active package satisfies (missing, disabled, replaced away, other version) and SHALL NOT enable dependencies itself; `enable ID --force` SHALL enable it anyway (the consumer is reported broken, "Broken consumers are reported, not enforced"). An already enabled package is not checked. Verified by `enable_refuses_inactive_dependencies_unless_forced` and `dropping_a_binding_checks_the_original_s_dependencies`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied

#### Scenario: Original with an inactive dependency
- **WHEN** `me/base` replaces `vendor/base`, `vendor/base` requires `lib ^1`, `lib` is disabled, and the user runs `disable me/base` or `replace vendor/base --with vendor/base`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes; `disable me/base --force` disables it anyway and `vendor/base` is reported broken

#### Scenario: Reactivated original
- **WHEN** dropping a binding makes ORIGINAL active again (`disable`, `forget` or `uninstall` of the replacing package, or `replace ORIGINAL --with ORIGINAL`)
- **THEN** ORIGINAL's direct dependencies are checked the same way: the removals refuse with `dependency_conflict` unless forced, `replace ORIGINAL --with ORIGINAL` refuses without a force bypass
