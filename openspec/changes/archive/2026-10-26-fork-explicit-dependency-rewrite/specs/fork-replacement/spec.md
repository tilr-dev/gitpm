## MODIFIED Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --into DIR` SHALL Git-clone the recorded installation source with history at installed revisions, into a new repository with a fresh UUID for every package and every package name preserved. It SHALL leave the installation unchanged, set the source as `upstream`, accept only installed IDs without `--from`, and exclude local installation work.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into hello-fork`
- **THEN** `hello-fork` is a Git clone whose `plugin.json` has `id` `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4`, the output contains `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 forked from 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (repository root)`, and `7e6e8392-7be8-55ae-bd03-2b430b1c5d59` is still installed
- **AND** the fork UUID is newly generated and differs from the original while the package name is preserved; repeating the fork into another new destination generates a different UUID

#### Scenario: Fork an uninstalled repository
- **WHEN** `gitpm fork 11111111-1111-4111-8111-111111111111 --from ./plugins --into ./my-plugins` is run without a workspace
- **THEN** it clones the newest stable `vVERSION` release whose commit contains `11111111-1111-4111-8111-111111111111`, assigns every package a fresh UUID preserving its name and preserves internal dependency IDs and requirements, preserving repository files and layout
- **AND** tags without that ID and legacy `ID@VERSION` tags are ignored; no workspace is opened, created or locked

#### Scenario: Explicit revision
- **WHEN** `--from SOURCE --ref REF` is supplied
- **THEN** any Git revision may be selected, even an untagged commit or branch; every supplied ID must exist at that commit

#### Scenario: Source package selection
- **WHEN** no ID is supplied for a source with multiple packages, or an ID does not exist at the selected revision
- **THEN** fork fails with `invalid_argument` listing the available packages and leaves no destination

### Requirement: Fork output names the next steps
The CLI SHALL print the fork lines, an optional explicit `gitpm fork-rewrite DIR` command before one `gitpm commit DIR NEW_VERSION` command to release all repository packages with one tag, then the explicit `gitpm install DIR ID` and `gitpm replace ORIGINAL --with ID` commands per package, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created
- **THEN** the output contains `Next steps (choose the packages to install and replace):` followed by the optional fork-rewrite command and the commit, install and replace commands

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
- **AND** library `fork_packages` and FFI `forkPackages` return the package list and the repository path; dependency rewrites are returned only by the separate library `fork_rewrite` / FFI `forkRewrite` call
- **AND** the host explicitly rewrites dependencies if desired, releases the repository and calls install and replace separately for the desired package

#### Scenario: Installed sibling remains unchanged
- **WHEN** `d63a8a6b-85d6-509c-87db-9a6096daa66c` and `e66d8976-2209-50e5-abaf-9360c0a03411` are installed and the caller explicitly releases, installs and replaces only `87fe9d68-adda-5200-92ad-a5c59d2264b9` after forking the repository, through either the CLI or library
- **THEN** `e66d8976-2209-50e5-abaf-9360c0a03411` remains unchanged and `e4e7959f-6315-5611-a719-75c5c2a920e3` is not automatically installed
