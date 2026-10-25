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

#### Scenario: Consumer references remain original UUIDs
- **WHEN** a consumer depends on an original UUID and `replace ORIGINAL --with FORK` activates a compatible fork with a different UUID
- **THEN** the consumer resolves through the replacement binding and its manifest and recorded dependency keys remain the original UUID; no consumer references are rewritten

### Requirement: Active fork rules
Single source for bindings when a package leaves or changes. Disabling, forgetting or uninstalling an active fork SHALL drop its binding and deploy ORIGINAL's `dist/` in the same transaction when ORIGINAL is active then (guards: dependencies). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the original SHALL keep the binding. A replacing package is always enabled ("Binding to a disabled package"). Verified by `a_binding_to_a_disabled_fork_is_invalid_state`.

#### Scenario: Binding to a disabled package
- **WHEN** `state.json` replaces `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` by the disabled `db33e43e-e652-5767-8e21-27fb3cef9723` (written by gitpm 0.5 `disable --force`, or edited by hand)
- **THEN** every command that reads the state (including `list`) fails with `invalid_data` naming both packages and the `replacements` entry to remove

#### Scenario: Fork release without replaces
- **WHEN** the fork's newer release no longer declares `replaces` for ORIGINAL
- **THEN** `update FORK` (and `rollback` to such a revision) fails with `version_conflict` ("… no longer declares that it replaces ORIGINAL …"), and `update all` keeps the fork back with the reason `no longer replaces ORIGINAL` (`HeldBack.reason` `no_longer_replaces`; `update_all_resolves_replacement_versions_together_with_consumers`)

#### Scenario: Uninstall the fork
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces `d135a7cf-1c63-549a-93cb-8f89656b2ced` and the user uninstalls `8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** the binding is dropped and `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is deployed again
- **AND** this is covered by `uninstalling_or_forgetting_the_replacement_deploys_the_original_again`

#### Scenario: Disable the fork
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces the installed `d135a7cf-1c63-549a-93cb-8f89656b2ced`, no consumer would break, and the user runs `disable 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** the binding is dropped, `dist/8f0b16d1-526c-56fd-bdae-b6b991bbcb76/` is removed and `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is deployed again; a later `enable 8f0b16d1-526c-56fd-bdae-b6b991bbcb76` does not restore the binding

#### Scenario: Reinstalled original stays replaced
- **WHEN** the original `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` is uninstalled while replaced and later installed again
- **THEN** it is still replaced by the fork until `replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9`

### Requirement: check shows fork upstream
For a fork, `check` SHALL add `upstream {id, base, latest, newer, source, problem}`: the original's latest release against the version the fork started from, taken from the catalog when it lists the original, otherwise from the release tags of `forkOf.source` (`source` `catalog` or `origin`); without either, `latest` is unknown. Verified by `check_shows_fork_upstream_and_other_catalogs` and `check::fork_upstream_from_fork_of_source`.

#### Scenario: Original has a newer release
- **WHEN** `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4` forked `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` 1.0.0 and the catalog has `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` 1.1.0
- **THEN** `check` prints `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4: fork of 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base`

#### Scenario: Upstream without a catalog
- **WHEN** no catalog is given and `forkOf.source` of `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4` has the tag `v1.1.0`
- **THEN** `check` prints `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4: fork of 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (base 1.0.0); original at origin: 1.1.0 — newer than the fork base`

#### Scenario: Unreadable upstream
- **WHEN** no catalog lists the original and `forkOf.source` cannot be read
- **THEN** `upstream.problem` says why and `check` prints `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4: fork of 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (base 1.0.0); original: unreachable (PROBLEM)`

### Requirement: Fork installs committed dist
Forking SHALL keep the existing installation model: install the fork's committed package dist without automatically building sources or enabling dev mode. Installation SHALL be a separate explicit `install` operation after the author explicitly releases the fork with `commit`; `fork` itself SHALL only prepare the editable repository. Replacement SHALL be a separate explicit `replace` operation.

#### Scenario: Install a fork with dist
- **WHEN** a package has source files and committed dist, and the user forks it, commits a new version and explicitly installs the fork
- **THEN** the fork contains the source tree and only its committed dist is deployed under the fork ID, without building sources or enabling dev mode

#### Scenario: Replace a repository with an uninstalled sibling
- **WHEN** only `d63a8a6b-85d6-509c-87db-9a6096daa66c` is installed from a repository that also contains `e66d8976-2209-50e5-abaf-9360c0a03411`, and the caller forks the repository with fresh UUIDs, commits a new version, explicitly installs only `87fe9d68-adda-5200-92ad-a5c59d2264b9` and replaces `d63a8a6b-85d6-509c-87db-9a6096daa66c` with `87fe9d68-adda-5200-92ad-a5c59d2264b9`, through either the CLI or library
- **THEN** `e4e7959f-6315-5611-a719-75c5c2a920e3` is not automatically installed and no binding is created for the uninstalled `e66d8976-2209-50e5-abaf-9360c0a03411`
- **AND** the existing dist and workspace layout rules for explicit install and replace apply

#### Scenario: Single-ID replacement API
- **WHEN** a host calls library `fork` or FFI `fork` with one installed ID from a multi-package repository
- **THEN** it receives the path to the editable repository containing all sibling packages, without releasing, installing or replacing any package
- **AND** library `fork_packages` and FFI `forkPackages` return the package list and dependency rewrites as well as the repository path
- **AND** the host releases the repository and calls install and replace separately for the desired package

#### Scenario: Installed sibling remains unchanged
- **WHEN** `d63a8a6b-85d6-509c-87db-9a6096daa66c` and `e66d8976-2209-50e5-abaf-9360c0a03411` are installed and the caller explicitly releases, installs and replaces only `87fe9d68-adda-5200-92ad-a5c59d2264b9` after forking the repository, through either the CLI or library
- **THEN** `e66d8976-2209-50e5-abaf-9360c0a03411` remains unchanged and `e4e7959f-6315-5611-a719-75c5c2a920e3` is not automatically installed

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
- **AND** one commit `Fork A, B as c03512e7-72ca-5ded-bf03-46cf85e45b85, 78e03c22-1ef1-5de5-a1c5-1c0c3c3d81d3` records all renamed packages without running user hooks

### Requirement: Fork copies an installed package
`fork ID [ID...] --into DIR` SHALL Git-clone the recorded installation source with history at installed revisions, into a new repository with a fresh UUID for every package and every package name preserved. It SHALL leave the installation unchanged, set the source as `upstream`, accept only installed IDs, and exclude local installation work.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into hello-fork`
- **THEN** `hello-fork` is a Git clone whose `plugin.json` has `id` `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4`, the output contains `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 forked from 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (repository root)`, and `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` is still installed

#### Scenario: Fresh fork identity
- **WHEN** the same source is forked twice into separate destinations
- **THEN** each fork has a newly generated UUID distinct from the original and from the other fork, while the package name is preserved

### Requirement: Fork requires explicit namespace and destination
`fork` SHALL require explicit `--into DIR` and SHALL NOT accept `--ns`. DIR MUST be new, outside the workspace and the source repository, without `..`. Existing destinations SHALL be refused unchanged. A failed fork SHALL remove directories it created.

#### Scenario: Removed namespace option
- **WHEN** `fork` receives `--ns NAMESPACE`
- **THEN** it fails as a usage error and creates nothing

#### Scenario: Missing destination
- **WHEN** `fork UUID` omits `--into`
- **THEN** it fails with a usage error and creates nothing

#### Scenario: Existing destination
- **WHEN** the destination already exists
- **THEN** fork refuses it and preserves its contents

#### Scenario: --version without --replace
- **WHEN** `fork UUID --into DIR` receives `--replace`, `--version 2.0.0` or `--force`
- **THEN** the CLI refuses the option before creating the fork or changing the workspace

#### Scenario: Destination inside the workspace
- **WHEN** `--into` points inside the workspace root
- **THEN** the fork fails with `invalid_argument` ("fork destination must be outside the managed workspace") and nothing is created

#### Scenario: Fork preserves inherited tags and workspace state
- **WHEN** an installed source repository contains packages A and B and the inherited tag `v0.2.0`, and a caller forks A through the CLI or library
- **THEN** the fork contains both packages with fresh UUIDs, source history and inherited tags, with `forkOf` and `replaces` metadata
- **AND** fork creates no release tag and does not install packages, change replacement bindings, update workspace state or reconcile dist
- **AND** a later explicit `commit` with an existing release tag still fails with a tag conflict

#### Scenario: Same namespace
- **WHEN** a package is forked without `--ns` into a new destination
- **THEN** its alias stays the same and the fork receives a fresh UUID, without namespace comparison

#### Scenario: Missing --ns
- **WHEN** `fork UUID --into DIR` receives no namespace option
- **THEN** the fork succeeds with a generated UUID and the preserved package name

### Requirement: Fork metadata
The fork manifest SHALL get `forkOf` `{id, version, commit, source}` and `replaces` `{id, version}` from its revision of the cloned source (the installed revision for installed packages from that source, the starting revision for other packages); `source` SHALL be the recorded source. Metadata and unknown keys SHALL be kept. HEAD SHALL be attached to a new branch `fork` (`fork-2`, … when taken).

#### Scenario: Fork manifest
- **WHEN** `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` 1.0.0, installed from `https://git.example/acme/hello.git`, is forked into a new repository
- **THEN** the fork's `plugin.json` has `forkOf.id` `7e6e8392-7be8-55ae-bd03-2b430b1c5d59`, `forkOf.version` `1.0.0`, `forkOf.source` `https://git.example/acme/hello.git` and `replaces` `{"id": "7e6e8392-7be8-55ae-bd03-2b430b1c5d59", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`
