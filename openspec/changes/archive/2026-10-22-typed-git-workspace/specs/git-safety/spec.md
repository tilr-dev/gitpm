## ADDED Requirements

### Requirement: Typed workspace Git operations
Production workspace management, including shared clones and forks, SHALL perform Git operations through typed methods of the internal transport trait. These callers SHALL NOT construct Git command argument lists or invoke raw Git or HTTP transport helpers. Existing behavior, output interpretation, SSH policy and error handling SHALL be preserved. Test fixture setup is outside this requirement.

#### Scenario: Workspace revisions and shared clones
- **WHEN** a workspace installs, updates, restores or removes packages and maintains shared clone refs
- **THEN** object queries, ancestry checks, checkout and ref maintenance use typed transport operations
- **AND** revision selection, retained refs and transaction behavior remain unchanged

#### Scenario: Fork an installed source
- **WHEN** installed packages are forked into a new repository
- **THEN** fetching missing revisions, selecting trees, configuring submodules, creating a branch and committing identities use typed transport operations
- **AND** the existing branch naming, history checks, hook policy and fork contents are preserved

#### Scenario: Export committed package files
- **WHEN** workspace management exports a package folder or its distribution tree
- **THEN** typed transport operations read object types, tree entries and blob bytes
- **AND** byte contents, executable permissions and existing symlink and submodule handling are preserved
