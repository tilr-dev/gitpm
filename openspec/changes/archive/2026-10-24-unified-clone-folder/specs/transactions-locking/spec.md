## MODIFIED Requirements

### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`repos`, `dist`, `dist/<ns>`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed` and `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Symlinked namespace
- **WHEN** `repos` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `dan/x` refuse and nothing outside the workspace root changes

#### Scenario: Blocked dist namespace
- **WHEN** `dist/acme` is a file or symlink and `acme/greeter` is active
- **THEN** an operation that deploys or removes `acme/greeter` fails with `workspace` and does not write or delete through `dist/acme`, while a command on an unrelated package can proceed
