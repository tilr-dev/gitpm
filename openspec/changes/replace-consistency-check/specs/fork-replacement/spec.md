## MODIFIED Requirements

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single binding `state.replacements[ORIGINAL] = FORK`, so all consumers resolve ORIGINAL through FORK at its `replaces.version`. Before any mutation, replace SHALL check the entire resulting active workspace. Any missing, disabled or version-incompatible dependency SHALL refuse the operation without changing state, clones or dist. Replace SHALL NOT install or enable dependencies and SHALL have no confirmation or force bypass.

#### Scenario: Missing dependency
- **WHEN** a fork needs an absent plugin
- **THEN** replace fails with `dependency_conflict` naming the consumer and dependency, and the workspace remains unchanged

#### Scenario: Compatible fork
- **WHEN** the proposed binding leaves every active dependency satisfied
- **THEN** replace applies the binding and dist changes in one transaction

#### Scenario: Provider preconditions
- **WHEN** a forward replacement is requested
- **THEN** both packages MUST be installed; FORK MUST be enabled and declare replaces.id equal to ORIGINAL

#### Scenario: Missing original
- **WHEN** ORIGINAL is not installed
- **THEN** replace refuses with `not_installed` and changes nothing

### Requirement: Compatibility is checked by versions only
Consumers whose requirement the provider's declared version does not satisfy SHALL be listed in `ReplacementPlan.incompatible`, and replace SHALL fail with `incompatible_replacement`. Compatibility SHALL be checked from metadata, without running package code. There SHALL be no confirm parameter, TTY prompt or `--force` for replace.

#### Scenario: Incompatible consumer
- **WHEN** app needs ORIGINAL ^2 and FORK declares replaces.version 1.4.0
- **THEN** check reports app and its requirement, replace fails with `incompatible_replacement`, and the workspace is unchanged

#### Scenario: Force refused
- **WHEN** a caller passes `--force` to replace
- **THEN** the CLI rejects the option without changing the workspace

### Requirement: Self-replacement and cycles are refused
`replace ORIGINAL --with ORIGINAL` SHALL remove the replacement binding, rather than create a self-binding. Replacement cycles SHALL be refused with `invalid_argument`: when FORK is itself replaced, ORIGINAL already replaces another package, or FORK depends on ORIGINAL.

#### Scenario: Cycle
- **WHEN** a is replaced by b and the caller tries to replace b with a
- **THEN** replace fails with `invalid_argument` and changes nothing

### Requirement: Switching back
Returning to the original SHALL be ordinary `replace ORIGINAL --with ORIGINAL`, removing its binding with the same workspace consistency check and atomicity as forward replacement. ORIGINAL MUST be installed and enabled. The old fork SHALL stay installed and enabled. There SHALL be no separate `--back`, `switch_back` or `switch_back_plan`, no confirmation and no force bypass.

#### Scenario: Return to original
- **WHEN** FORK replaces the installed enabled ORIGINAL and the resulting workspace is consistent
- **THEN** replace with ORIGINAL removes the binding, redeploys ORIGINAL and reports the former provider

#### Scenario: Original unavailable
- **WHEN** ORIGINAL is absent or disabled
- **THEN** returning refuses without changing the binding or dist; the caller must first install or enable ORIGINAL

#### Scenario: Return breaks versions or dependencies
- **WHEN** returning makes a consumer version incompatible or exposes an unsatisfied dependency of ORIGINAL
- **THEN** replace refuses and the workspace remains unchanged

#### Scenario: No binding
- **WHEN** ORIGINAL is installed and enabled but has no binding
- **THEN** replace with ORIGINAL still checks consistency and succeeds without creating a self-binding

### Requirement: Active fork rules
Single source for bindings when a package leaves or changes. Disabling, forgetting or uninstalling an active fork SHALL drop its binding and deploy ORIGINAL's `dist/` in the same transaction when ORIGINAL is active then (guards: dependencies). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the original SHALL keep the binding. A replacing package is always enabled ("Binding to a disabled package"). Verified by `a_binding_to_a_disabled_fork_is_invalid_state`.

#### Scenario: Binding to a disabled package
- **WHEN** `state.json` replaces `vendor/base` by the disabled `mine/base` (written by gitpm 0.5 `disable --force`, or edited by hand)
- **THEN** every command that reads the state (including `list`) fails with `invalid_data` naming both packages and the `replacements` entry to remove

#### Scenario: Fork release without replaces
- **WHEN** the fork's newer release no longer declares `replaces` for ORIGINAL
- **THEN** `update FORK` (and `rollback` to such a revision) fails with `version_conflict` ("… no longer declares that it replaces ORIGINAL …"), and `update all` keeps the fork back with the reason `no longer replaces ORIGINAL` (`HeldBack.reason` `no_longer_replaces`; `update_all_resolves_replacement_versions_together_with_consumers`)

#### Scenario: Uninstall the fork
- **WHEN** `me/greeter` replaces `acme/greeter` and the user uninstalls `me/greeter`
- **THEN** the binding is dropped and `dist/acme/greeter/` is deployed again
- **AND** this is covered by `uninstalling_or_forgetting_the_replacement_deploys_the_original_again`

#### Scenario: Disable the fork
- **WHEN** `me/greeter` replaces the installed `acme/greeter`, no consumer would break, and the user runs `disable me/greeter`
- **THEN** the binding is dropped, `dist/me/greeter/` is removed and `dist/acme/greeter/` is deployed again; a later `enable me/greeter` does not restore the binding

#### Scenario: Reinstalled original stays replaced
- **WHEN** the original `vendor/base` is uninstalled while replaced and later installed again
- **THEN** it is still replaced by the fork until `replace vendor/base --with vendor/base`

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` returned by `replace_plan` and `replace` SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible` and `installed_version`, allowing output without another state read. `replace_check` SHALL additionally report workspace dependency issues and missing dependency installation candidates in its consistency report.

#### Scenario: Original return plan
- **WHEN** FORK replaces ORIGINAL 1.4.0 and a caller plans replacement with ORIGINAL
- **THEN** provider is ORIGINAL, current is FORK and installed_version is 1.4.0

#### Scenario: Distinct provider and declared versions
- **WHEN** FORK 0.3.0 declares replaces.version 1.4.0
- **THEN** the plan carries installed_version 0.3.0 and version 1.4.0

## ADDED Requirements

### Requirement: Explicit workspace replacement check
`Manager::replace_check(original, replacement)` SHALL resolve the entire proposed active workspace from metadata without changing state, clones or dist. It SHALL return the replacement plan, every unsatisfied dependency with consumer, dependency and version requirement, and the missing dependencies to install. Catalog candidates MAY be used for missing dependencies, including transitives; installed versions SHALL remain fixed. Existing update `check` is unchanged.

#### Scenario: Missing plugin
- **WHEN** a fork requires a plugin absent from the workspace
- **THEN** the check reports the missing dependency and what must be installed, without installing it

#### Scenario: Whole workspace
- **WHEN** another active package has an unsatisfied dependency unrelated to the new mapping
- **THEN** check reports it too and replace refuses

#### Scenario: Disabled or incompatible installed provider
- **WHEN** a provider is installed but disabled or has an incompatible version
- **THEN** check reports it as a problem requiring explicit user action

#### Scenario: Changed requirement
- **WHEN** the fork requires a version incompatible with an installed provider
- **THEN** check reports the mismatch without changing installed versions

### Requirement: Explicit missing dependency resolution
`Manager::replace_resolve(original, replacement)` SHALL recompute the proposed check against current locked state and explicitly install missing dependencies from the configured catalog in a transaction. It SHALL NOT update or enable installed packages or apply the replacement binding. If no consistent solution exists, it SHALL refuse without changes. Replace SHALL recheck rather than trust an earlier report.

#### Scenario: Check resolve replace
- **WHEN** check finds installable missing dependencies, the caller invokes resolve, then replace
- **THEN** resolve installs the missing dependencies including transitives without applying the mapping, and replace subsequently succeeds

#### Scenario: Stale check
- **WHEN** workspace metadata changes after a successful check
- **THEN** resolve and replace use current state and refuse if it is inconsistent

#### Scenario: Unsatisfiable missing dependencies
- **WHEN** the catalog cannot provide compatible releases
- **THEN** resolve refuses without partial installs or binding changes
