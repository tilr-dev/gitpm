## MODIFIED Requirements

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set `state.replacements[ORIGINAL] = FORK`, so consumers resolve ORIGINAL at FORK's `replaces.version`. Before mutation it SHALL check the resulting active workspace. Only the provider's transitive dependencies and active consumers' requirements on ORIGINAL SHALL block it. Failure SHALL leave state, clones and dist unchanged. Replace SHALL NOT install or enable dependencies or have a confirmation or force bypass.

#### Scenario: Fork with unsatisfiable dependencies
- **WHEN** a fork needs an absent plugin
- **THEN** replace fails with `dependency_conflict` naming the consumer and dependency, and the workspace remains unchanged

#### Scenario: Compatible fork
- **WHEN** the proposed provider has satisfied dependencies transitively and active consumers accept its offered version
- **THEN** replace applies the binding and dist changes in one transaction

#### Scenario: Fork not usable as the replacing package
- **WHEN** a forward replacement is requested
- **THEN** both packages MUST be installed; FORK MUST be enabled and declare replaces.id equal to ORIGINAL

#### Scenario: Original not installed
- **WHEN** ORIGINAL is not installed
- **THEN** replace refuses with `not_installed` and changes nothing

#### Scenario: Effect on dist
- **WHEN** replace succeeds
- **THEN** dist changes as specified by "Replace removes the original's dist" in the same transaction

#### Scenario: Output variants
- **WHEN** the provider's installed version differs from its declared replaces.version or there are no consumers
- **THEN** output uses `ORIGINAL replaced by FORK V (as ORIGINAL V') for: …` or ends `for: no consumers yet`

#### Scenario: Replace for all consumers
- **WHEN** app1 and app2 depend on ORIGINAL and replace with FORK succeeds
- **THEN** both resolve to FORK and output names both consumers

### Requirement: Compatibility is checked by versions only
Consumers whose requirement the provider's declared version does not satisfy SHALL be listed in `ReplacementPlan.incompatible`, and replace SHALL fail with `incompatible_replacement`. Compatibility SHALL be checked from metadata, without running package code. There SHALL be no confirm parameter, TTY prompt or `--force` for replace.

#### Scenario: Incompatible consumer without a TTY
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

#### Scenario: Back with the original absent
- **WHEN** ORIGINAL is absent or disabled
- **THEN** returning refuses without changing the binding or dist; the caller must first install or enable ORIGINAL

#### Scenario: Return breaks versions or dependencies
- **WHEN** returning makes a consumer version incompatible or exposes an unsatisfied dependency of ORIGINAL
- **THEN** replace refuses and the workspace remains unchanged

#### Scenario: Back without a replacement
- **WHEN** ORIGINAL is installed and enabled but has no binding
- **THEN** replace with ORIGINAL still checks consistency and succeeds without creating a self-binding; the CLI reports `ORIGINAL already uses original VERSION`

#### Scenario: Output variants
- **WHEN** replace ORIGINAL --with ORIGINAL removes a binding
- **THEN** output is `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)`; without consumers it says `no installed consumers`

#### Scenario: Back to a disabled original
- **WHEN** replace ORIGINAL --with ORIGINAL is requested while ORIGINAL is disabled
- **THEN** it refuses with invalid_argument telling the caller to enable ORIGINAL, without changing the binding or dist

#### Scenario: Consumers the original does not serve
- **WHEN** returning would leave consumers without an installed, enabled, compatible original
- **THEN** it refuses without changes or a force bypass; absent originals use not_installed and version-incompatible consumers use incompatible_replacement

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

#### Scenario: Switch back
- **WHEN** FORK replaces ORIGINAL 1.4.0 and a caller plans replacement with ORIGINAL
- **THEN** provider is ORIGINAL, current is FORK and installed_version is 1.4.0

#### Scenario: Fork version differs from the declared one
- **WHEN** FORK 0.3.0 declares replaces.version 1.4.0
- **THEN** the plan carries installed_version 0.3.0 and version 1.4.0

#### Scenario: Switch back to a disabled fork
- **WHEN** a host reads replace_plan(ORIGINAL, ORIGINAL) for returning from an active fork
- **THEN** current names the enabled fork; no fork state needs reading because disabling it would have dropped the binding

#### Scenario: Plan for a missing original
- **WHEN** ORIGINAL was uninstalled while replaced and a host calls replace_plan(ORIGINAL, ORIGINAL)
- **THEN** planning refuses with not_installed rather than returning a plan to drop the binding

## ADDED Requirements

### Requirement: Explicit workspace replacement check
`Manager::replace_check(original, replacement)` SHALL check the proposed active workspace without mutations. It SHALL return the replacement plan, all unsatisfied dependencies with consumer and requirement, and missing dependencies for the proposed provider's closure. Catalog candidates MAY supply missing dependencies, including transitives; installed versions SHALL remain fixed. Unrelated existing breakage SHALL be diagnostic only. Existing update `check` is unchanged.

#### Scenario: Missing plugin
- **WHEN** a fork requires a plugin absent from the workspace
- **THEN** the check reports the missing dependency and what must be installed, without installing it

#### Scenario: Whole workspace
- **WHEN** another active package has an unsatisfied dependency unrelated to the new mapping
- **THEN** check reports it too, but compatible replace succeeds and resolve does not reinstall the unrelated missing dependency, even if that broken package also consumes ORIGINAL

#### Scenario: Disabled or incompatible installed provider
- **WHEN** a provider is installed but disabled or has an incompatible version
- **THEN** check reports it as a problem requiring explicit user action

#### Scenario: Changed requirement
- **WHEN** the fork requires a version incompatible with an installed provider
- **THEN** check reports the mismatch without changing installed versions

### Requirement: Explicit missing dependency resolution
`Manager::replace_resolve(original, replacement)` SHALL recompute the check under lock and transactionally install missing dependencies only for the proposed provider and its transitive dependencies. It SHALL NOT update or enable installed packages or apply the binding. Unsatisfiable replacement constraints SHALL refuse without changes. Replace SHALL recheck current state.

#### Scenario: Check resolve replace
- **WHEN** check finds installable missing dependencies, the caller invokes resolve, then replace
- **THEN** resolve installs the missing dependencies including transitives without applying the mapping, and replace subsequently succeeds

#### Scenario: Stale check
- **WHEN** workspace metadata changes after a successful check
- **THEN** resolve and replace use current state and refuse if the replacement dependencies or consumers of ORIGINAL are incompatible

#### Scenario: Unsatisfiable missing dependencies
- **WHEN** the catalog cannot provide compatible releases
- **THEN** resolve refuses without partial installs or binding changes

#### Scenario: Disabled transitive provider
- **WHEN** the proposed provider depends on an installed package whose dependency is disabled
- **THEN** resolve and replace refuse without changing files, installing dependencies or enabling packages
