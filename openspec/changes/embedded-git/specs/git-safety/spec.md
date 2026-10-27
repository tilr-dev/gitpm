## MODIFIED Requirements

### Requirement: One subprocess boundary
All Git access of the library and all its subprocesses SHALL go through one internal module; Git is implemented there with system `git` or with an embedded library, HTTP with system `curl`; no other library module SHALL start processes or open network connections. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any gitpm command
- **THEN** the CLI sets `Options.custom_ssh` (an FFI host sets `ManagerOptions.customSsh` and passes it to `authCheck`), gitpm then sets no SSH command of its own, and the system-git backend's subprocesses use the inherited variable

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

#### Scenario: Mac without Command Line Tools
- **WHEN** the embedded git backend is built in and `git` is not installed
- **THEN** install and update work and no `git` subprocess is started
