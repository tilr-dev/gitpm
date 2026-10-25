## MODIFIED Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --ns NAMESPACE --into DIR` SHALL Git-clone the recorded installation source with history at installed revisions, into a new repository with every package renamed to `NAMESPACE/<same name>`. It SHALL leave the installation unchanged, set the source as `upstream`, accept only installed IDs without `--from`, and exclude local installation work.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into hello-fork`
- **THEN** `hello-fork` is a Git clone whose `plugin.json` has `id` `me/hello`, the output contains `me/hello forked from acme/hello (repository root)`, and `acme/hello` is still installed

#### Scenario: Fork an uninstalled repository
- **WHEN** `gitpm fork acme/theme --from ./plugins --ns me --into ./my-plugins` is run without a workspace
- **THEN** it clones the newest stable `vVERSION` release whose commit contains `acme/theme`, renames every package and rewrites internal dependencies, preserving repository files and layout
- **AND** tags without that ID and legacy `ID@VERSION` tags are ignored; no workspace is opened, created or locked

#### Scenario: Explicit revision
- **WHEN** `--from SOURCE --ref REF` is supplied
- **THEN** any Git revision may be selected, even an untagged commit or branch; every supplied ID must exist at that commit

#### Scenario: Source package selection
- **WHEN** no ID is supplied for a source with multiple packages, or an ID does not exist at the selected revision
- **THEN** fork fails with `invalid_argument` listing the available packages and leaves no destination

### Requirement: Fork metadata
The fork manifest SHALL get `forkOf` `{id, version, commit, source}` and `replaces` `{id, version}` from its revision of the cloned source (the installed revision for installed packages from that source, the starting revision for other packages); with `--from`, all packages SHALL record the resolved commit; `source` SHALL be the recorded source or canonicalized explicit source. Metadata and unknown keys SHALL be kept. HEAD SHALL be attached to a new branch `fork` (`fork-2`, … when taken).

#### Scenario: Fork manifest
- **WHEN** `acme/hello` 1.0.0, installed from `https://git.example/acme/hello.git`, is forked into namespace `me`
- **THEN** the fork's `plugin.json` has `forkOf.id` `acme/hello`, `forkOf.version` `1.0.0`, `forkOf.source` `https://git.example/acme/hello.git` and `replaces` `{"id": "acme/hello", "version": "1.0.0"}`
- **AND** metadata keys are carried unchanged; this is covered by `metadata::fork_and_commit_carry_the_metadata_unchanged`

### Requirement: Fork isolates source and destination state
With `--from SOURCE`, fork SHALL clone the explicit source without reading or locking a workspace, exclude uncommitted source edits, and reject destinations inside a local source. Without `--from`, fork SHALL exclude local installation work and use only the recorded source at installed revisions. It SHALL reject unsupported recorded sources and existing destinations without changing the installation or an existing destination.

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
