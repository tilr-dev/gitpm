## MODIFIED Requirements

### Requirement: Active fork rules
Single source for what happens to a replacement binding when a package leaves or changes. Disabling, forgetting or uninstalling an active fork SHALL drop its binding and deploy ORIGINAL's `dist/` again in the same transaction when ORIGINAL is active then; the guard is the one rule of dependencies ("Dependents guard removal"), and ORIGINAL's own dependencies are checked as in dependencies ("enable needs active dependencies"). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the original SHALL keep the binding. A replacing package is always enabled: a `state.json` that binds ORIGINAL to a disabled package (produced by gitpm 0.5 or earlier, whose forced disable kept the binding, or by a hand edit) SHALL be refused with `invalid_data`; the way out is to remove the entry from `replacements`. Verified by `a_binding_to_a_disabled_fork_is_invalid_state`.

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
- **THEN** it is still replaced by the fork until `replace vendor/base --back`

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single replacement binding `state.replacements[ORIGINAL] = FORK`, after which every consumer of ORIGINAL, existing and future, resolves to FORK, seen by the solver as ORIGINAL at FORK's `replaces.version`. Preconditions (single source): FORK MUST be installed, enabled and declare `replaces.id` equal to ORIGINAL, and ORIGINAL MUST be installed. Nothing SHALL be redirected without it. Verified by `fork_replacement_main_scenario` and `replace_requires_the_original_and_back_drops_an_absent_one`.

#### Scenario: Effect on dist
- **WHEN** the replace succeeds
- **THEN** `dist/` changes as specified in dist ("Replace removes the original's dist")
- **AND** this is covered by `required_global_fork_replacement_through_the_cli`

#### Scenario: Fork not usable as the replacing package
- **WHEN** FORK is not installed, is disabled, or does not declare `replaces.id` equal to ORIGINAL
- **THEN** the replace fails with `not_installed`, or `invalid_argument` ("FORK is disabled; enable it first" / "FORK does not declare that it replaces ORIGINAL …")

#### Scenario: Original not installed
- **WHEN** `vendor/base` is not installed and the user runs `replace vendor/base --with mine/base`
- **THEN** the replace fails with `not_installed` naming `vendor/base` and nothing changes

#### Scenario: Fork with unsatisfiable dependencies
- **WHEN** FORK's own dependencies would not be satisfied in the resulting workspace
- **THEN** the replace fails with `dependency_conflict` ("FORK has unsatisfied dependencies: …")

#### Scenario: Output variants
- **WHEN** the declared `replaces.version` differs from the installed version, or nothing depends on ORIGINAL
- **THEN** the line is `ORIGINAL replaced by FORK V (as ORIGINAL V') for: …`, or ends `for: no consumers yet`

#### Scenario: Replace for all consumers
- **WHEN** `app1` and `app2` depend on `vendor/base` and the user replaces it with `mine/base`
- **THEN** both resolve to `mine/base` and the CLI prints `vendor/base replaced by mine/base V for: app1, app2`

### Requirement: Switching back
`replace ORIGINAL --back` (`Manager::switch_back`) SHALL return all consumers to the installed original under the same confirmation rule and print `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)`. Consumers ORIGINAL does not serve once the binding is dropped (ORIGINAL not installed or disabled; a version mismatch is the confirmation rule) SHALL refuse with `has_dependents` naming them unless forced (`--force`, `switch_back(original, confirm, force)`), the one rule of dependencies ("Dependents guard removal"). When ORIGINAL is not installed it SHALL drop only the binding and and print `ORIGINAL switched back from FORK (ORIGINAL is not installed; broken: consumers)` or `(ORIGINAL is not installed; no installed consumers)`. Replacing again with another fork SHALL switch the single binding. Verified by `fork_replace_of_two_packages_replaces_both_and_switches_back` and `replace_requires_the_original_and_back_drops_an_absent_one`.

#### Scenario: Output variants
- **WHEN** no package depends on ORIGINAL
- **THEN** the line says `no installed consumers`

#### Scenario: Back with the original absent
- **WHEN** `vendor/base` was uninstalled while replaced by `mine/base`, `app` depends on `vendor/base`, and the user runs `replace vendor/base --back`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` the binding is dropped and `app` is reported broken

#### Scenario: Back to a disabled original
- **WHEN** `mine/base` replaces the installed but disabled `vendor/base`, `app` depends on `vendor/base`, and the user runs `replace vendor/base --back`
- **THEN** it fails with `has_dependents` naming `app` (also when incompatible consumers were confirmed); with `--force` the binding is dropped and `app` is reported broken
- **AND** this is covered by `switch_back_to_a_disabled_original_is_guarded`

#### Scenario: Back without a replacement
- **WHEN** nothing replaces `vendor/base` and the user runs `replace vendor/base --back`
- **THEN** the command fails with `invalid_argument` ("vendor/base is not replaced")

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` (of `replace_plan`, `switch_back_plan`, `replace`, `switch_back` and pending fork replacements) SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible`, and `installed_version` (the installed version of `provider`), so the replace and switch-back lines are built from the plan without reading state. When switching back to an ORIGINAL that is not installed, `installed_version` is absent, `version` is empty and `incompatible` is empty (the consumers are broken, not incompatible). Verified by `replacement_plan_carries_installed_version`.

#### Scenario: Switch back
- **WHEN** `me/greeter` replaces the installed `acme/greeter` 1.4.0 and the user runs `replace acme/greeter --back`
- **THEN** the plan has `current` `me/greeter` and `installed_version` `1.4.0`, and the line ends `; me/greeter stays installed and enabled (disable or uninstall it if not wanted)`

#### Scenario: Switch back to a disabled fork
- **WHEN** a host reads `switch_back_plan` to build the switch-back line
- **THEN** the plan carries no fork state: the fork named in `current` is always enabled (disabling it drops the binding, "Active fork rules"), so the line always ends `; FORK stays installed and enabled (disable or uninstall it if not wanted)`

#### Scenario: Plan for a missing original
- **WHEN** `vendor/base` was uninstalled while replaced by `mine/base` and a host calls `switch_back_plan("vendor/base")`
- **THEN** the plan has `provider` `vendor/base`, an empty `version`, no `installed_version`, `current` `mine/base`, every consumer of `vendor/base` in `consumers` and none in `incompatible`

#### Scenario: Fork version differs from the declared one
- **WHEN** `me/greeter` 0.3.0 declares `replaces: acme/greeter 1.4.0` and the user replaces `acme/greeter` with it
- **THEN** the plan has `version` `1.4.0` and `installed_version` `0.3.0`, and the line is `acme/greeter replaced by me/greeter 0.3.0 (as acme/greeter 1.4.0) for: …`
