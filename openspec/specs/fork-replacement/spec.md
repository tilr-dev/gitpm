# fork-replacement Specification

## Purpose

Defines forking an installed package into an author repository under a new namespace, and the explicit, global replacement of an original by a fork for every consumer, including the version-based compatibility check and switching back.

## Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --ns NAMESPACE --into DIR` SHALL Git-clone the recorded installation source with history at installed revisions, into a new repository with every package renamed to `NAMESPACE/<same name>`. It SHALL leave the installation unchanged, set the source as `upstream`, accept only installed IDs, and exclude local installation work.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into hello-fork`
- **THEN** `hello-fork` is a Git clone whose `plugin.json` has `id` `me/hello`, the output contains `me/hello forked from acme/hello (repository root)`, and `acme/hello` is still installed

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

### Requirement: Fork metadata
The fork manifest SHALL get `forkOf` `{id, version, commit, source}` and `replaces` `{id, version}` from its revision of the cloned source (the installed revision for installed packages from that source, the starting revision for other packages); `source` SHALL be the recorded source. Metadata and unknown keys SHALL be kept. HEAD SHALL be attached to a new branch `fork` (`fork-2`, … when taken).

#### Scenario: Fork manifest
- **WHEN** `acme/hello` 1.0.0, installed from `https://git.example/acme/hello.git`, is forked into namespace `me`
- **THEN** the fork's `plugin.json` has `forkOf.id` `acme/hello`, `forkOf.version` `1.0.0`, `forkOf.source` `https://git.example/acme/hello.git` and `replaces` `{"id": "acme/hello", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`

### Requirement: Fork output names the next steps
The CLI SHALL print the fork lines and the next steps per package (the strings below are the common forms): `gitpm commit DIR NEW_VERSION --package ID` (commit local edits, tag), `gitpm install DIR ID`, `gitpm replace ORIGINAL --with ID`, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created
- **THEN** the output contains `Next steps (choose the packages to install and replace):` followed by the commit, install and replace commands

### Requirement: Replacement is explicit and global
`replace ORIGINAL --with FORK` (`Manager::replace`) SHALL set the single binding `state.replacements[ORIGINAL] = FORK`, so all consumers resolve ORIGINAL through FORK at its `replaces.version`. Before any mutation, replace SHALL check the entire resulting active workspace. Any missing, disabled or version-incompatible dependency SHALL refuse the operation without changing state, clones or dist. Replace SHALL NOT install or enable dependencies and SHALL have no confirmation or force bypass.

#### Scenario: Fork with unsatisfiable dependencies
- **WHEN** a fork needs an absent plugin
- **THEN** replace fails with `dependency_conflict` naming the consumer and dependency, and the workspace remains unchanged

#### Scenario: Compatible fork
- **WHEN** the proposed binding leaves every active dependency satisfied
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
- **THEN** replace with ORIGINAL still checks consistency and succeeds without creating a self-binding

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

### Requirement: Fork preserves source submodules
A fork SHALL preserve submodules outside dist as Git links at their recorded commits with the corresponding `.gitmodules` configuration, including when a requested package comes from an older installed revision. The user SHALL be able to populate them with `git submodule update --init`; package files SHALL NOT be exported through archive.

#### Scenario: Source submodule
- **WHEN** an installed package's recorded source has a submodule outside dist
- **THEN** its fork retains the gitlink and configuration and initializing it checks out the recorded submodule commit

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
- **WHEN** a host calls library `fork_in_namespace` or FFI `fork` with one installed ID from a multi-package repository
- **THEN** it receives the path to the editable repository containing all sibling packages, without releasing, installing or replacing any package
- **AND** library `fork_packages` and FFI `forkPackages` return the package list and dependency rewrites as well as the repository path
- **AND** the host releases the repository and calls install and replace separately for the desired package

#### Scenario: Installed sibling remains unchanged
- **WHEN** `demo/a` and `demo/b` are installed and the caller explicitly releases, installs and replaces only `my/a` after forking the repository, through either the CLI or library
- **THEN** `demo/b` remains unchanged and `my/b` is not automatically installed

### Requirement: Fork isolates source and destination state
Fork SHALL exclude local installation work and use only the recorded source at installed revisions. It SHALL reject unsupported recorded sources and existing destinations without changing the installation or an existing destination.

#### Scenario: Unsupported recorded source
- **WHEN** installation metadata names a remote helper or a relative source
- **THEN** fork refuses it with `invalid_argument` and leaves no destination

#### Scenario: Installed clone has local work
- **WHEN** the installed clone has local commits and staged, unstaged or untracked edits
- **THEN** the fork uses the recorded source at the installed revision and leaves all local work in the installation unchanged

#### Scenario: Existing destination
- **WHEN** `--into` names an existing directory, including an existing multi-package fork
- **THEN** the command fails with `invalid_argument` and leaves it unchanged

#### Scenario: Repository starting revision
- **WHEN** one or more IDs identify a source with packages installed at different revisions
- **THEN** the fork starts at the newest installed revision of all packages from that source, restores every installed package folder at its recorded revision, and keeps uninstalled packages at the starting revision
- **AND** one commit `Fork A, B as me/A, me/B` records all renamed packages without running user hooks

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
