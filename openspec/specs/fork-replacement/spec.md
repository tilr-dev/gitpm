# fork-replacement Specification

## Purpose

Defines forking an installed package into an author repository under a new namespace, and the explicit, global replacement of an original by a fork for every consumer, including the version-based compatibility check and switching back.

## Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --ns NAMESPACE --into DIR` SHALL copy installed packages (with Git history and local edits of the installed clone) into the fork repository `DIR` under the IDs `NAMESPACE/<same name>`; the original installation SHALL be unchanged and the original repository SHALL become the `upstream` remote. Only installed IDs are accepted. Verified by `fork_preserves_history_edits_and_original_installation`.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into hello-fork`
- **THEN** `hello-fork` is a Git repository whose `plugin.json` has `id` `me/hello`, the output contains `me/hello forked from acme/hello (repository root)`, and `acme/hello` is still installed

### Requirement: Fork requires explicit namespace and destination
`fork` SHALL require `--ns NAMESPACE` and `--into DIR`; nothing SHALL be derived from an ID. `DIR` MUST be a new folder or an existing repository with `gitpm.json`; anything else, a `DIR` inside the workspace, a path with `..`, the source repository itself or a folder inside it SHALL be refused, and a failed new fork SHALL leave no folder behind. Verified by `fork_rejects_managed_destinations_and_unknown_plugins` and `workspace::fork_refusals_create_nothing`.

#### Scenario: Same namespace
- **WHEN** the user runs `gitpm fork acme/hello --ns acme --into x`
- **THEN** it fails with `invalid_argument` ("fork needs a different namespace")

#### Scenario: --version without --replace
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into x --version 2.0.0`
- **THEN** it fails with `--version is not valid for fork` and creates nothing

#### Scenario: Missing --ns
- **WHEN** the user runs `gitpm fork acme/hello --into x`
- **THEN** it fails with `fork needs --ns NAMESPACE: …` and creates nothing

#### Scenario: Destination inside the workspace
- **WHEN** `--into` points inside the workspace root
- **THEN** the fork fails with `invalid_argument` ("fork destination must be outside the managed workspace") and nothing is created

### Requirement: Fork metadata
The fork's manifest SHALL get `forkOf` `{id, version, commit}` of the forked revision and `replaces` `{id, version}` set to the forked version; metadata and unknown keys SHALL be kept (package-manifest, "Unknown keys are preserved"; plugin-metadata, "Metadata survives rewrites"). The fork SHALL be on a new branch `fork` (or `fork-2`, `fork-3`, … when that name is taken) with attached HEAD. Verified by `fork_records_local_head_preserves_index_and_immediate_parent`.

#### Scenario: Fork manifest
- **WHEN** `acme/hello` 1.0.0 is forked into namespace `me`
- **THEN** the fork's `plugin.json` has `forkOf.id` `acme/hello`, `forkOf.version` `1.0.0` and `replaces` `{"id": "acme/hello", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`

### Requirement: Fork output names the next steps
Without `--replace`, the CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits, tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created without `--replace`
- **THEN** the output contains `Next steps (or all at once: add --replace):` followed by the commit, install and replace commands

### Requirement: Fork and replace in one step
`fork … --replace [--version V] [--force]` SHALL make one release commit of the forked folders (setting `version` when `--version` is given) and one release tag per package (`NAMESPACE/NAME@VERSION`, package-ids), then install each fork (dependencies first) and replace its original. If the release fails nothing SHALL be installed; a failed install or replace of one package SHALL NOT stop the others. Verified by `fork_replace_releases_installs_and_replaces_in_one_step`.

#### Scenario: One-step fork
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into f --replace`
- **THEN** `me/hello@<forked version>` is tagged, installed and replaces `acme/hello`, so only `dist/me/hello/` is deployed

### Requirement: Fork folder is never removed after a partial failure
When a step of `fork --replace` fails, the fork folder SHALL be kept and the steps that succeeded SHALL stay: the library reports the failed step in its result instead of failing, an exception to errors ("Failing operations leave the workspace unchanged"). The CLI SHALL print per package which step failed and how to finish or undo, and SHALL fail with exit status 1 and the first failing step's `Error kind`. Verified by `fork_replace_keeps_the_fork_and_says_how_to_finish_when_a_step_fails`.

#### Scenario: Failed step in the library result
- **WHEN** the install step of `Manager::fork_replace` (or `fork_packages_replace`) fails
- **THEN** the call returns a result whose `failed` step and error describe it (FFI `forkAndReplace`/`forkPackagesAndReplace`: `failedStep`, `errorKind`, `errorMessage`), and the release commit and tag stay

#### Scenario: Install step fails
- **WHEN** the release succeeds but installing the fork fails
- **THEN** stderr says `ID is released but not installed. To finish:` with the install and replace commands, and the fork folder exists

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single replacement binding `state.replacements[ORIGINAL] = FORK`, after which every consumer of ORIGINAL, existing and future, resolves to FORK, seen by the solver as ORIGINAL at FORK's `replaces.version`. Preconditions (single source): FORK MUST be installed, enabled and declare `replaces.id` equal to ORIGINAL; ORIGINAL need not be installed. Nothing SHALL be redirected without it. Verified by `fork_replacement_main_scenario`.

#### Scenario: Effect on dist
- **WHEN** the replace succeeds
- **THEN** `dist/` changes as specified in dist ("Replace removes the original's dist")
- **AND** this is covered by `required_global_fork_replacement_through_the_cli`

#### Scenario: Fork not usable as the replacing package
- **WHEN** FORK is not installed, is disabled, or does not declare `replaces.id` equal to ORIGINAL
- **THEN** the replace fails with `not_installed`, or `invalid_argument` ("FORK is disabled; enable it first" / "FORK does not declare that it replaces ORIGINAL …")

#### Scenario: Fork with unsatisfiable dependencies
- **WHEN** FORK's own dependencies would not be satisfied in the resulting workspace
- **THEN** the replace fails with `dependency_conflict` ("FORK has unsatisfied dependencies: …")

#### Scenario: Output variants
- **WHEN** the declared `replaces.version` differs from the installed version, or nothing depends on ORIGINAL
- **THEN** the line is `ORIGINAL replaced by FORK V (as ORIGINAL V') for: …`, or ends `for: no consumers yet`

#### Scenario: Replace for all consumers
- **WHEN** `app1` and `app2` depend on `vendor/base` and the user replaces it with `mine/base`
- **THEN** both resolve to `mine/base` and the CLI prints `vendor/base replaced by mine/base V for: app1, app2`

### Requirement: Compatibility is checked by versions only
Consumers whose requirement the fork's declared version does not satisfy SHALL be listed in `ReplacementPlan.incompatible`, and the replace SHALL fail with `incompatible_replacement` unless confirmed. The CLI SHALL ask `Replace anyway? [y/N]` on a TTY and otherwise require `--force` (the warning is still printed). Code SHALL never be run to check compatibility. Verified by `replacement_compatibility_needs_confirmation`.

#### Scenario: Incompatible consumer without a TTY
- **WHEN** `app` needs `vendor/base ^2` and `mine/base` declares `replaces.version` `1.4.0`, and stdin is not a terminal
- **THEN** the CLI warns `app needs vendor/base ^2`, prints `No terminal to confirm; use --force to replace anyway.` and fails with `incompatible_replacement`

### Requirement: Self-replacement and cycles are refused
Replacing a package with itself, or creating a replacement cycle, SHALL always be refused with `invalid_argument`, even when confirmed or forced: "cannot replace ORIGINAL by itself", and "cyclic replacement: …" when FORK is itself replaced, when ORIGINAL already replaces another package, or when FORK depends on ORIGINAL.

#### Scenario: Cycle
- **WHEN** `a` is replaced by `b` and the user tries to replace `b` with `a`
- **THEN** the replace fails with `invalid_argument` ("cyclic replacement: …")

### Requirement: Switching back
`replace ORIGINAL --back` (`Manager::switch_back`) SHALL return all consumers to the installed original under the same confirmation rule and print `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)`. It SHALL fail with `not_installed` when ORIGINAL is not installed. Replacing again with another fork SHALL switch the single binding. Verified by `fork_replace_of_two_packages_replaces_both_and_switches_back`.

#### Scenario: Output variants
- **WHEN** no package depends on ORIGINAL, or FORK is disabled or gone
- **THEN** the line says `no installed consumers`, ends `; FORK stays installed (disabled)`, or has no FORK clause

#### Scenario: Back without a replacement
- **WHEN** nothing replaces `vendor/base` and the user runs `replace vendor/base --back`
- **THEN** the command fails with `invalid_argument` ("vendor/base is not replaced")

### Requirement: Active fork rules
Single source for what happens to a replacement binding when a package leaves or changes. Disabling, forgetting or uninstalling an active fork is guarded as in dependencies ("Dependents guard removal"). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the fork SHALL drop its binding and deploy ORIGINAL's `dist/` again in the same transaction when ORIGINAL is active then; uninstalling or forgetting the original SHALL keep the binding.

#### Scenario: Fork release without replaces
- **WHEN** the fork's newer release no longer declares `replaces` for ORIGINAL
- **THEN** `update FORK` (and `rollback` to such a revision) fails with `version_conflict` ("… no longer declares that it replaces ORIGINAL …"), and `update all` reports the fork kept back

#### Scenario: Uninstall the fork
- **WHEN** `me/greeter` replaces `acme/greeter` and the user uninstalls `me/greeter`
- **THEN** the binding is dropped and `dist/acme/greeter/` is deployed again
- **AND** this is covered by `uninstalling_or_forgetting_the_replacement_deploys_the_original_again`

#### Scenario: Reinstalled original stays replaced
- **WHEN** the original `vendor/base` is uninstalled while replaced and later installed again
- **THEN** it is still replaced by the fork until `replace vendor/base --back`

### Requirement: check shows fork upstream
For a fork, `check` SHALL add `upstream {id, base, latest, newer}`: the original's latest catalog release against the version the fork started from. Verified by `check_shows_fork_upstream_and_other_catalogs`.

#### Scenario: Original has a newer release
- **WHEN** `me/hello` forked `acme/hello` 1.0.0 and the catalog has `acme/hello` 1.1.0
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base`

### Requirement: A pending replacement of fork --replace is confirmed and resumed
When the replace step of `fork --replace` needs confirmation (incompatible consumers, not forced), the library SHALL stop that package after its install and return its `ReplacementPlan` as pending; `Manager::fork_replace_resume` (FFI `forkReplaceResume`) SHALL apply the pending replacements once confirmed, re-planning under the lock (`incompatible_replacement` when the consumers changed). Verified by `fork_replace_pending_replacement_is_resumed_after_confirmation`.

#### Scenario: Confirmed on a TTY
- **WHEN** `gitpm fork acme/hello --ns me --into f --replace` finds `app` needing `acme/hello ^2` and the user answers `y`
- **THEN** the CLI prints the warning from the pending plan, asks on stderr and calls the resume API, and `acme/hello` is replaced by `me/hello`

#### Scenario: No terminal
- **WHEN** the same command runs without a TTY and without `--force`
- **THEN** `me/hello` stays released and installed, `acme/hello` is not replaced, and the command fails with `incompatible_replacement`

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` (of `replace_plan`, `switch_back_plan`, `replace`, `switch_back` and pending fork replacements) SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible`, `installed_version` (the installed version of `provider`) and `fork_enabled` (FORK is installed and enabled), so the replace and switch-back lines are built from the plan without reading state. Verified by `replacement_plan_carries_installed_version_and_fork_enabled`.

#### Scenario: Switch back to a disabled fork
- **WHEN** `me/greeter` is disabled and the user runs `replace acme/greeter --back`
- **THEN** the plan has `fork_enabled` false and the line ends `; me/greeter stays installed (disabled)`

#### Scenario: Fork version differs from the declared one
- **WHEN** `me/greeter` 0.3.0 declares `replaces: acme/greeter 1.4.0` and the user replaces `acme/greeter` with it
- **THEN** the plan has `version` `1.4.0` and `installed_version` `0.3.0`, and the line is `acme/greeter replaced by me/greeter 0.3.0 (as acme/greeter 1.4.0) for: …`
