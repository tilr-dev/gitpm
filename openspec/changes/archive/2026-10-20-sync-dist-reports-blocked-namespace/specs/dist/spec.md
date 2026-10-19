## MODIFIED Requirements

### Requirement: Unknown paths are left alone
Folders under `dist/` that belong to no known package SHALL be left untouched and reported (the CLI warns on stderr). A namespace path under `dist/` that is a symlink or a file SHALL be reported the same way (in `unmanaged`) and nothing SHALL be written below it; an active package below such a path SHALL be listed in `skipped` with the reason. Verified by `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Namespace path that is a file
- **WHEN** `dist/acme` is a file and `acme/greeter` is active
- **THEN** a command on an unrelated package succeeds and leaves `acme/greeter` undeployed, while an operation that deploys or removes `acme/greeter` fails with `workspace` (transactions-locking, "Managed paths are never followed through symlinks")

#### Scenario: sync-dist with a blocked namespace
- **WHEN** `dist/acme` is a file, `acme/greeter` is active and the user runs `sync-dist`
- **THEN** `acme/greeter` is in `skipped` ("dist/acme is not a folder") and `sync-dist` fails with `not_deployed` instead of printing `dist/ already holds exactly the active packages.`

#### Scenario: Unknown folder
- **WHEN** `dist/custom/` exists and belongs to no package
- **THEN** a mutating command leaves it and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`
