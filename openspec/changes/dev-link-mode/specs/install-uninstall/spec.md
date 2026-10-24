## ADDED Requirements

### Requirement: Explicit local linking is distinct from branch following
gitpm SHALL provide a library link operation and equivalent FFI and CLI entry points for a local author package. It SHALL validate identity and resolve the installation folder before publishing a symlink or changing state, using the same `plugin.json.dist` selection as managed installation (`"dist"` when omitted, `"."` for the package root). It MUST NOT redefine `--dev`, which continues to follow the origin default branch.

#### Scenario: Link a package folder
- **WHEN** a valid local package folder is linked
- **THEN** gitpm records the package in link mode and publishes its installation folder through a symlink, without creating a managed clone or executing package code

#### Scenario: Multi-package repository
- **WHEN** the selected local package belongs to a multi-package repository
- **THEN** the link identifies that package and resolves its installation folder relative to its package folder

#### Scenario: Explicitly link a prepared fork
- **WHEN** the author prepares a repository with `gitpm fork acme/hello --ns me --into hello-fork` and then explicitly links its `me/hello` package
- **THEN** only that selected package is installed in link mode, without requiring a new release or changing the original's installation or replacement bindings
- **AND** fork preparation alone neither installs the fork nor publishes a linked deployment; sibling packages remain uninstalled

#### Scenario: Explicitly replace with a linked fork
- **WHEN** an installed, enabled linked fork declares a validated `replaces.id` matching the original and the user runs `replace ORIGINAL --with FORK`
- **THEN** the existing explicit replacement operation reads the link record's top-level `replaces` offer and applies the existing replacement and dependency rules
- **AND** consumers of the original use the linked fork's live deployment; linking alone never creates that binding

### Requirement: Invalid link targets leave no changes
Linking SHALL refuse invalid manifests with `invalid_manifest` (unparsable JSON with `invalid_data`) and missing installation folders, conflicting deployment entries, non-UTF-8 paths, workspace cycles or targets in workspace managed storage with `workspace`, without changing state or the author folder.

#### Scenario: Missing build output
- **WHEN** the manifest's installation folder does not exist
- **THEN** linking fails with `workspace`, leaving state and the author tree unchanged, without running a build

#### Scenario: Invalid manifest
- **WHEN** the package manifest has no valid identifier
- **THEN** linking fails with `invalid_manifest`, leaving state and the author tree unchanged

#### Scenario: Unrelated deployment entry
- **WHEN** the selected package ID has an unowned deployment entry, including a dangling symlink
- **THEN** linking refuses with `workspace`, preserving that entry, state and the external folder

#### Scenario: Path cannot be represented in state
- **WHEN** a canonical package root or installation-folder path contains non-UTF-8 bytes
- **THEN** linking refuses with `workspace` before staging, preserving state and external files rather than recording a lossy path

### Requirement: Rollback refuses linked packages
`rollback` of a linked package SHALL fail with `no_previous_version`, leaving the symlink, state and the external folder unchanged: a link keeps no previous revision.

#### Scenario: Roll back a link
- **WHEN** rollback is requested for a linked package
- **THEN** it fails with `no_previous_version` and the symlink, state and external folder are unchanged

### Requirement: Link removal preserves all author work
For a link record, removal plans SHALL describe only owned workspace entries and state changes. The external repository and installation folder MUST NOT appear in `deletes` and MUST NOT be deleted, reset or cleaned, including with force or lost-work confirmation. External dirty files, staged files, ignored files, unpublished refs and stashes SHALL NOT prevent removal of the link, since removal cannot lose them. Managed-clone lost-work checks SHALL remain unchanged.

#### Scenario: Uninstall a dirty linked package
- **WHEN** a linked author's repository has staged edits, ignored output, an unpublished commit and a stash and the user uninstalls the package
- **THEN** only its workspace symlink and recorded package are removed, and the external files, refs and stash are unchanged

#### Scenario: Forced removal
- **WHEN** the user forces removal of a linked package
- **THEN** force does not authorize traversal or deletion of the link target
