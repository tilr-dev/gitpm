## MODIFIED Requirements

### Requirement: Empty parent folders are removed
Removing a package SHALL also remove the folders it leaves empty (`dist/<namespace>`, `repos`). Verified by `removal_leaves_no_empty_namespace_or_clone_folders`.

#### Scenario: Last package of a namespace
- **WHEN** the only package of namespace `acme` is uninstalled
- **THEN** `dist/acme/` and the unused `repos/` no longer exist
