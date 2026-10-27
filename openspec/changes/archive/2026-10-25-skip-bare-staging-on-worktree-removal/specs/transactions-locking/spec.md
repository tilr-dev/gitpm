## ADDED Requirements

### Requirement: Worktree removal avoids unnecessary bare copies
Removing a package worktree SHALL NOT stage a bare repository that is being deleted or has no registration for that package. Retained bare repositories without that registration SHALL remain in place.

#### Scenario: Last repository user is removed
- **WHEN** uninstall, forget of a missing worktree, or build cleanup removes the last user of a bare repository
- **THEN** the bare repository is deleted without creating a staged copy for worktree removal

#### Scenario: Build update retains a shared bare repository
- **WHEN** a build updates to another build while a sibling retains the source repository
- **AND** the build has no worktree registration
- **THEN** worktree cleanup does not copy or replace the retained bare repository
