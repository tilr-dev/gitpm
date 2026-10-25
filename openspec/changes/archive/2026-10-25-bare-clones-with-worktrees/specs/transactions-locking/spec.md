## MODIFIED Requirements

### Requirement: Transactions stage only touched paths
Mutations SHALL stage bare repositories, package worktrees, output and next state inside `.pm-txn`, leaving live paths untouched before commit. Commit SHALL journal changes, move replaced live paths to `.pm-txn/old`, publish staged paths and repair Git links, then atomically rename state last (errors, "Failing operations leave the workspace unchanged"). Untouched repositories SHALL NOT be rewritten except for keep-alive refs as described below.

#### Scenario: Keep-alive refs after commit
- **WHEN** a transaction changes a package of a shared clone
- **THEN** after commit its keep-alive refs are written in the live clone (multi-package-repos, "Keep-alive refs")

#### Scenario: Deployed folders
- **WHEN** any mutating operation commits
- **THEN** the deployed folders it adds or removes are those of dist ("Every mutating command reconciles all of dist")

#### Scenario: Forget in a shared clone
- **WHEN** forgetting a Git package that shares a bare repository
- **THEN** its worktree is retained and other worktrees stay unchanged

#### Scenario: Readers during commit
- **WHEN** another process reads `state.json` while a commit runs
- **THEN** it always sees a complete file

#### Scenario: Failure before commit
- **WHEN** an install fails while staging
- **THEN** nothing staged has been moved into the live workspace and `.pm-txn` is gone

#### Scenario: Relocated worktrees
- **WHEN** a staged bare repository or worktree is committed or recovered after interruption
- **THEN** worktree Git links resolve to live managed paths, all retained worktrees remain usable, and no link depends on a removed transaction directory


### Requirement: Managed paths are never followed through symlinks
Every directory between the root and a managed path (`repos`, `worktrees`, `worktrees/<ns>`, `dist`, `dist/<ns>`) MUST be a real directory; a symlinked or non-directory managed parent SHALL fail with `workspace` and nothing SHALL be written or deleted through it. Verified by `workspace::symlinked_namespace_directory_is_never_followed` and `a_namespace_path_that_is_not_a_folder_blocks_nothing_else`.

#### Scenario: Symlinked namespace
- **WHEN** `repos` is a symlink to a folder outside the workspace
- **THEN** install and `uninstall --force` of `dan/x` refuse and nothing outside the workspace root changes

#### Scenario: Blocked dist namespace
- **WHEN** `dist/acme` is a file or symlink and `acme/greeter` is active
- **THEN** an operation that deploys or removes `acme/greeter` fails with `workspace` and does not write or delete through `dist/acme`, while a command on an unrelated package can proceed

#### Scenario: Symlinked worktree parent
- **WHEN** `worktrees` or the affected `worktrees/<ns>` parent is a symlink or a non-directory
- **THEN** an operation using that path refuses with `workspace` and changes nothing through it
