## MODIFIED Requirements

### Requirement: Fork requires explicit namespace and destination
`fork` SHALL require explicit `--ns NAMESPACE` and `--into DIR`. DIR MUST be new, outside the workspace and the source repository, without `..`. Existing destinations SHALL be refused unchanged. A failed fork SHALL remove directories it created.

#### Scenario: Same namespace
- **WHEN** the user runs `gitpm fork acme/hello --ns acme --into x`
- **THEN** it fails with `invalid_argument` ("fork needs a different namespace")

#### Scenario: --version without --replace
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into x` with `--replace`, `--version 2.0.0` or `--force`
- **THEN** the CLI refuses the option before creating the fork or changing the workspace

#### Scenario: Missing --ns
- **WHEN** the user runs `gitpm fork acme/hello --into x`
- **THEN** it fails with `fork needs --ns NAMESPACE: …` and creates nothing

#### Scenario: Destination inside the workspace
- **WHEN** `--into` points inside the workspace root
- **THEN** the fork fails with `invalid_argument` ("fork destination must be outside the managed workspace") and nothing is created

#### Scenario: Fork preserves inherited tags and workspace state
- **WHEN** an installed source repository contains packages A and B and the inherited tag `v0.2.0`, and a caller forks A through the CLI or library
- **THEN** the fork contains both renamed packages, source history and inherited tags, with `forkOf` and `replaces` metadata
- **AND** fork creates no release tag and does not install packages, change replacement bindings, update workspace state or reconcile dist
- **AND** a later explicit `commit` with an existing release tag still fails with a tag conflict

### Requirement: Fork output names the next steps
The CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits, tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created
- **THEN** the output contains `Next steps (choose the packages to install and replace):` followed by the commit, install and replace commands

### Requirement: Switching back
`replace ORIGINAL --back` (`Manager::switch_back(original, confirm, force)`) SHALL return all consumers to the installed original (same confirmation rule) and print `ORIGINAL switched back from FORK (ORIGINAL V for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)`. Replacing again SHALL switch the binding. Verified by `replace_requires_the_original_and_back_drops_an_absent_one`.

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

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` (of `replace_plan`, `switch_back_plan`, `replace`, `switch_back`) SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible` and `installed_version` (the installed version of `provider`), so the replace and switch-back lines are built from the plan without reading state. Verified by `replacement_plan_carries_installed_version`.

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

### Requirement: Fork installs committed dist
Forking SHALL keep the existing installation model: install the fork's committed package dist without automatically building sources or enabling dev mode. Installation SHALL be a separate explicit `install` operation after the author explicitly releases the fork with `commit`; `fork` itself SHALL only prepare the editable repository. Replacement SHALL be a separate explicit `replace` operation.

#### Scenario: Install a fork with dist
- **WHEN** a package has source files and committed dist, and the user forks it, commits a new version and explicitly installs the fork
- **THEN** the fork contains the source tree and only its committed dist is deployed under the fork ID, without building sources or enabling dev mode

#### Scenario: Replace a repository with an uninstalled sibling
- **WHEN** only `demo/a` is installed from a repository that also contains `demo/b`, and the caller forks the repository into namespace `my`, commits a new version, explicitly installs only `my/a` and replaces `demo/a` with `my/a`, through either the CLI or library
- **THEN** `my/b` is not automatically installed and no binding is created for the uninstalled `demo/b`
- **AND** the existing dist and workspace layout rules for explicit install and replace apply

#### Scenario: Single-ID replacement API
- **WHEN** a host calls the ordinary library fork API or FFI `fork` with one installed ID from a multi-package repository
- **THEN** it receives only the editable repository's fork data, including its sibling packages, without releasing, installing or replacing any package
- **AND** the host releases the repository and calls install and replace separately for the desired package

#### Scenario: Installed sibling remains unchanged
- **WHEN** `demo/a` and `demo/b` are installed and the caller explicitly releases, installs and replaces only `my/a` after forking the repository, through either the CLI or library
- **THEN** `demo/b` remains unchanged and `my/b` is not automatically installed

## REMOVED Requirements

### Requirement: Fork and replace in one step
**Reason**: Fork only prepares the editable repository; release, installation and replacement are explicit operations.
**Migration**: Run `fork`, `commit`, `install` and `replace` separately.

### Requirement: Fork folder is never removed after a partial failure
**Reason**: Fork only prepares the editable repository; release, installation and replacement are explicit operations.
**Migration**: Run `fork`, `commit`, `install` and `replace` separately.

### Requirement: A pending replacement of fork --replace is confirmed and resumed
**Reason**: Fork only prepares the editable repository; release, installation and replacement are explicit operations.
**Migration**: Run `fork`, `commit`, `install` and `replace` separately.
