## MODIFIED Requirements

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


## ADDED Requirements

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
