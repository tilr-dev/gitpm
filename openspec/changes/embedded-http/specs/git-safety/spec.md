## MODIFIED Requirements

### Requirement: One subprocess boundary
All HTTP access of the library and all its subprocesses SHALL go through one internal module; HTTP is implemented there with system `curl` or with an embedded client (ureq), Git with system `git`; no other library module SHALL start processes or open network connections. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any gitpm command
- **THEN** the CLI sets `Options.custom_ssh` (an FFI host sets `ManagerOptions.customSsh` and passes it to `authCheck`), gitpm then sets no SSH command of its own, and git uses the inherited variable

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

#### Scenario: Catalog without curl
- **WHEN** the embedded HTTP backend is built in and `curl` is not installed
- **THEN** catalog reads work and no `curl` subprocess is started
