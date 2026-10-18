## MODIFIED Requirements

### Requirement: Active fork rules
Single source for what happens to a replacement binding when a package leaves or changes. Disabling, forgetting or uninstalling an active fork SHALL drop its binding and deploy ORIGINAL's `dist/` again in the same transaction when ORIGINAL is active then; the guard is the one rule of dependencies ("Dependents guard removal"). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the original SHALL keep the binding.

#### Scenario: Fork release without replaces
- **WHEN** the fork's newer release no longer declares `replaces` for ORIGINAL
- **THEN** `update FORK` (and `rollback` to such a revision) fails with `version_conflict` ("… no longer declares that it replaces ORIGINAL …"), and `update all` keeps the fork back with the reason `no longer replaces ORIGINAL`

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
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single replacement binding `state.replacements[ORIGINAL] = FORK`, after which every consumer of ORIGINAL, existing and future, resolves to FORK, seen by the solver as ORIGINAL at FORK's `replaces.version`. Preconditions (single source): FORK MUST be installed, enabled and declare `replaces.id` equal to ORIGINAL, and ORIGINAL MUST be installed. Nothing SHALL be redirected without it. Verified by `fork_replacement_main_scenario`.

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
`replace ORIGINAL --back` (`Manager::switch_back`) SHALL return all consumers to the installed original under the same confirmation rule and print `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled`. When ORIGINAL is not installed it SHALL drop the binding (`has_dependents` unless forced when consumers would break). Replacing with another fork SHALL switch the binding. Verified by `fork_replace_of_two_packages_replaces_both_and_switches_back`.

#### Scenario: Output variants
- **WHEN** no package depends on ORIGINAL, or FORK is disabled or gone
- **THEN** the line says `no installed consumers`, ends `; FORK stays installed (disabled)`, or has no FORK clause

#### Scenario: Back with the original absent
- **WHEN** `vendor/base` was uninstalled while replaced by `mine/base`, `app` depends on `vendor/base`, and the user runs `replace vendor/base --back`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` the binding is dropped and `app` is reported broken

#### Scenario: Back without a replacement
- **WHEN** nothing replaces `vendor/base` and the user runs `replace vendor/base --back`
- **THEN** the command fails with `invalid_argument` ("vendor/base is not replaced")

