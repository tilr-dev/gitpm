# Workspace layout changes

Changes to bare repository storage, package worktrees or transaction activation must
run the complete Dagger integration suite before review and merge. The default
workflow checks do not include it.

In a task folder with the workflow wrappers, run `bin/check dt` after committing the
final changes, then `bin/check --verify --need dt` to verify the default checks and
the Dagger result belong to that revision. After a merge or any further code change,
run the gate for the new revision. In a standalone clone, run
`env -u SSH_AUTH_SOCK ./itests/dt` alongside the required CI checks.

See [itests/README.md](itests/README.md) for engine setup and layout assertions.
