## MODIFIED Requirements

### Requirement: Fork output names the next steps
The CLI SHALL print the fork lines, a `gitpm packages DIR` inspection command and a hint for `gitpm dependency` edits before one `gitpm commit DIR NEW_VERSION` command to release all repository packages with one tag, then the explicit `gitpm install DIR ID` and `gitpm replace ORIGINAL --with ID` commands per package, and for a new fork `gitpm remote DIR GIT_URL`.

#### Scenario: Next steps after fork
- **WHEN** a fork is created
- **THEN** the output contains `Next steps (choose the packages to install and replace):` followed by the package inspection command and dependency editing hint and the commit, install and replace commands

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
- **AND** library `fork_packages` and FFI `forkPackages` return the package list and the repository path; dependency edits are made only through the explicit package dependency API
- **AND** the host explicitly edits dependencies if desired, releases the repository and calls install and replace separately for the desired package

#### Scenario: Installed sibling remains unchanged
- **WHEN** `d63a8a6b-85d6-509c-87db-9a6096daa66c` and `e66d8976-2209-50e5-abaf-9360c0a03411` are installed and the caller explicitly releases, installs and replaces only `87fe9d68-adda-5200-92ad-a5c59d2264b9` after forking the repository, through either the CLI or library
- **THEN** `e66d8976-2209-50e5-abaf-9360c0a03411` remains unchanged and `e4e7959f-6315-5611-a719-75c5c2a920e3` is not automatically installed
