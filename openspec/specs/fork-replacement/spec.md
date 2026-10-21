# fork-replacement Specification

## Purpose

Defines forking an installed package into an author repository under a new namespace, and the explicit, global replacement of an original by a fork for every consumer, including the version-based compatibility check and switching back.

## Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --ns NAMESPACE --into DIR` SHALL Git-clone the recorded installation source with history at installed revisions, into a new repository with IDs `NAMESPACE/<same name>`. It SHALL leave the installation unchanged, set the source as `upstream`, accept only installed IDs, and exclude local installation work.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into hello-fork`
- **THEN** `hello-fork` is a Git clone whose `plugin.json` has `id` `me/hello`, the output contains `me/hello forked from acme/hello (repository root)`, and `acme/hello` is still installed

#### Scenario: Unsupported recorded source
- **WHEN** installation metadata names a remote helper or a relative source
- **THEN** fork refuses it with `invalid_argument` and leaves no destination

#### Scenario: Installed clone has local work
- **WHEN** the installed clone has local commits and staged, unstaged or untracked edits
- **THEN** the fork uses the recorded source at the installed revision and leaves all local work in the installation unchanged

### Requirement: Fork requires explicit namespace and destination
`fork` SHALL require explicit `--ns NAMESPACE` and `--into DIR`. DIR MUST be new, outside the workspace and the source repository, without `..`. Existing destinations SHALL be refused unchanged. A failed fork SHALL remove directories it created.

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

#### Scenario: Existing destination
- **WHEN** `--into` names an existing directory, including an existing multi-package fork
- **THEN** the command fails with `invalid_argument` and leaves it unchanged

### Requirement: Fork metadata
The fork manifest SHALL get `forkOf` `{id, version, commit, source}` and `replaces` `{id, version}` from the installed revision of the cloned source; `source` SHALL be the recorded source. Metadata and unknown keys SHALL be kept. HEAD SHALL be attached to a new branch `fork` (`fork-2`, … when taken).

#### Scenario: Fork manifest
- **WHEN** `acme/hello` 1.0.0, installed from `https://git.example/acme/hello.git`, is forked into namespace `me`
- **THEN** the fork's `plugin.json` has `forkOf.id` `acme/hello`, `forkOf.version` `1.0.0`, `forkOf.source` `https://git.example/acme/hello.git` and `replaces` `{"id": "acme/hello", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`

### Requirement: Fork output names the next steps
Without `--replace`, the CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits, tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created without `--replace`
- **THEN** the output contains `Next steps (or all at once: add --replace):` followed by the commit, install and replace commands

### Requirement: Fork and replace in one step
`fork … --replace [--version V] [--force]` SHALL release the forked folders under the rules of `commit` (authoring): one commit (`version` set by `--version`) and one tag per package (`NAMESPACE/NAME@VERSION`, package-ids), then install each fork (dependencies first) and replace its original. If the release fails nothing SHALL be installed; a failed install or replace of one package SHALL NOT stop the others. Verified by `fork_replace_releases_installs_and_replaces_in_one_step`.

#### Scenario: Release without file transport
- **WHEN** a multi-package fork is released while Git's file transport is disabled
- **THEN** all release tags are created at the release commit without using a transport
- **AND** verified by `fork_replace_release_without_file_transport`

#### Scenario: One-step fork
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into f --replace`
- **THEN** `me/hello@<forked version>` is tagged, installed and replaces `acme/hello`, so only `dist/me/hello/` is deployed

#### Scenario: Failed release commit
- **WHEN** a pre-commit hook rejects the release commit of `fork … --replace`
- **THEN** the forked manifests and the index are as before the release, no tag exists (tags are created all or none), nothing is installed, and the result reports the failed `release` step
- **AND** a wrong-typed metadata key fails the release step with `invalid_manifest` (plugin-metadata)
- **AND** verified by `fork_replace_release_rejecting_hook_restores_manifests_and_index` and `fork_replace_release_rejects_wrong_metadata_type`

### Requirement: Fork folder is never removed after a partial failure
When a step of `fork --replace` fails, the fork folder SHALL be kept and the steps that succeeded SHALL stay: the library reports the failed step in its result instead of failing, an exception to errors ("Failing operations leave the workspace unchanged"). The CLI SHALL print per package which step failed and how to finish or undo, and SHALL fail with exit status 1 and the first failing step's `Error kind`. Verified by `fork_replace_keeps_the_fork_and_says_how_to_finish_when_a_step_fails`.

#### Scenario: Failed step in the library result
- **WHEN** the install step of `Manager::fork_replace` (or `fork_packages_replace`) fails
- **THEN** the call returns a result whose `failed` step and error describe it (FFI `forkAndReplace`/`forkPackagesAndReplace`: `failedStep`, `errorKind`, `errorMessage`), and the release commit and tag stay

#### Scenario: Install step fails
- **WHEN** the release succeeds but installing the fork fails
- **THEN** stderr says `ID is released but not installed. To finish:` with the install and replace commands, and the fork folder exists

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single binding `state.replacements[ORIGINAL] = FORK`, after which every consumer of ORIGINAL, existing and future, resolves to FORK (the solver sees ORIGINAL at `replaces.version`). Preconditions (single source): FORK MUST be installed, enabled and declare `replaces.id` equal to ORIGINAL; ORIGINAL MUST be installed. Verified by `fork_replacement_main_scenario` and `replace_requires_the_original_and_back_drops_an_absent_one`.

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
`replace ORIGINAL --back` (`Manager::switch_back(original, confirm, force)`) SHALL return all consumers to the installed original (same confirmation rule) and print `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)`. Replacing again SHALL switch the binding. Verified by `fork_replace_of_two_packages_replaces_both_and_switches_back` and `replace_requires_the_original_and_back_drops_an_absent_one`.

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

#### Scenario: Consumers the original does not serve
- **WHEN** ORIGINAL is not installed or disabled once the binding is dropped (a version mismatch is the confirmation rule instead)
- **THEN** the consumers it leaves broken refuse with `has_dependents` naming them unless forced (`--force`), the one rule of dependencies ("Dependents guard removal")
- **AND** for an ORIGINAL that is not installed only the binding is dropped and the line is `ORIGINAL switched back from FORK (ORIGINAL is not installed; broken: consumers)` or `(ORIGINAL is not installed; no installed consumers)`

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
- **THEN** it is still replaced by the fork until `replace vendor/base --back`

### Requirement: check shows fork upstream
For a fork, `check` SHALL add `upstream {id, base, latest, newer, source, problem}`: the original's latest release against the version the fork started from, taken from the catalog when it lists the original, otherwise from the release tags of `forkOf.source` (`source` `catalog` or `origin`); without either, `latest` is unknown. Verified by `check_shows_fork_upstream_and_other_catalogs` and `check::fork_upstream_from_fork_of_source`.

#### Scenario: Original has a newer release
- **WHEN** `me/hello` forked `acme/hello` 1.0.0 and the catalog has `acme/hello` 1.1.0
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base`

#### Scenario: Upstream without a catalog
- **WHEN** no catalog is given and `forkOf.source` of `me/hello` has the tag `acme/hello@1.1.0`
- **THEN** `check` prints `me/hello: fork of acme/hello (base 1.0.0); original at origin: 1.1.0 — newer than the fork base`

#### Scenario: Unreadable upstream
- **WHEN** no catalog lists the original and `forkOf.source` cannot be read
- **THEN** `upstream.problem` says why and `check` prints `me/hello: fork of acme/hello (base 1.0.0); original: unreachable (PROBLEM)`

### Requirement: A pending replacement of fork --replace is confirmed and resumed
When the replace step of `fork --replace` needs confirmation (incompatible consumers, not forced), the library SHALL stop that package after its install and return its `ReplacementPlan` as pending; `Manager::fork_replace_resume` (FFI `forkReplaceResume`) SHALL apply the pending replacements once confirmed, re-planning under the lock (`incompatible_replacement` when the consumers changed). Verified by `fork_replace_pending_replacement_is_resumed_after_confirmation`.

#### Scenario: Confirmed on a TTY
- **WHEN** `gitpm fork acme/hello --ns me --into f --replace` finds `app` needing `acme/hello ^2` and the user answers `y`
- **THEN** the CLI prints the warning from the pending plan, asks on stderr and calls the resume API, and `acme/hello` is replaced by `me/hello`

#### Scenario: No terminal
- **WHEN** the same command runs without a TTY and without `--force`
- **THEN** `me/hello` stays released and installed, `acme/hello` is not replaced, and the command fails with `incompatible_replacement`

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` (of `replace_plan`, `switch_back_plan`, `replace`, `switch_back` and pending fork replacements) SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible` and `installed_version` (the installed version of `provider`), so the replace and switch-back lines are built from the plan without reading state. Verified by `replacement_plan_carries_installed_version`.

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

### Requirement: Fork preserves source submodules
A fork SHALL preserve submodules outside dist as Git links at their recorded commits with the corresponding `.gitmodules` configuration, including when a selected package comes from an older installed revision. The user SHALL be able to populate them with `git submodule update --init`; package files SHALL NOT be exported through archive.

#### Scenario: Source submodule
- **WHEN** an installed package's recorded source has a submodule outside dist
- **THEN** its fork retains the gitlink and configuration and initializing it checks out the recorded submodule commit

### Requirement: Fork installs committed dist
Forking SHALL keep the existing installation model: install the fork's committed package dist without automatically building sources or enabling dev mode.

#### Scenario: Install a fork with dist
- **WHEN** a package has source files and committed dist and the user forks it with `--replace`
- **THEN** the fork contains the source tree and only its committed dist is deployed under the fork ID
