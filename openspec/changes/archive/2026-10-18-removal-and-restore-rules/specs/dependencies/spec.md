## MODIFIED Requirements

### Requirement: Dependents guard removal
Single source for the `has_dependents` guard: `disable`, `forget` and `uninstall` of a package SHALL refuse with `has_dependents` unless forced exactly when an installed package's dependency would break: one that resolves to the package directly, or one that resolves to it through a replacement binding and that ORIGINAL does not satisfy (installed, enabled, matching version) once the binding is dropped. `dependents` and `UninstallPlan.dependents` SHALL name exactly those consumers. With force each SHALL be allowed. Switching back to an ORIGINAL that is not installed or disabled follows the same rule (fork-replacement, "Switching back"). Verified by `dependents_guard_disable_forget_and_uninstall` and `removing_an_active_fork_guards_only_consumers_the_original_breaks`.

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

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every dependency of ID that no active package satisfies (missing, disabled, replaced away or of a non-matching version) and SHALL NOT enable dependencies itself; only direct dependencies are checked. `enable ID --force` SHALL enable it anyway; the consumer is then reported broken ("Broken consumers are reported, not enforced"). An already enabled package is not checked. The same check SHALL apply to ORIGINAL when dropping a binding makes it active again: `disable`, `forget` and `uninstall` of the replacing package refuse with `dependency_conflict` unless forced, and `replace ORIGINAL --back` refuses with `dependency_conflict`. Verified by `enable_refuses_inactive_dependencies_unless_forced` and `dropping_a_binding_checks_the_original_s_dependencies`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied

#### Scenario: Original with an inactive dependency
- **WHEN** `me/base` replaces `vendor/base`, `vendor/base` requires `lib ^1`, `lib` is disabled, and the user runs `disable me/base` or `replace vendor/base --back`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes; `disable me/base --force` disables it anyway and `vendor/base` is reported broken
