## MODIFIED Requirements

### Requirement: Git never waits for input
Every git command on a repository and the `auth check` ls-remote SHALL run with closed stdin, `GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS`, `credential.interactive=never`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20` and one batch SSH command ("Batch SSH command") unless `Options.custom_ssh` is set or Git config defines `core.sshCommand`; one still running after 10 minutes SHALL be killed and reported as `git` ("timed out after 600s"). Verified by `exec::git_never_reads_stdin_or_prompts`.

#### Scenario: Wedged git
- **WHEN** a git subprocess hangs for 10 minutes
- **THEN** it is killed and the operation fails with `git`
- **AND** this is covered by `exec::wedged_subprocess_is_killed_at_the_limit`

#### Scenario: User's GIT_SSH_COMMAND
- **WHEN** the inherited environment has `GIT_SSH_COMMAND`
- **THEN** git uses it over the batch `core.sshCommand`, because git gives the variable precedence; the library does not read it

#### Scenario: Batch SSH command
- **WHEN** the user's Git config has no `core.sshCommand` and the environment has no `GIT_SSH_COMMAND`
- **THEN** clones, fetches and `auth check` alike get `ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o StrictHostKeyChecking=accept-new`

#### Scenario: Access check ls-remote
- **WHEN** `auth check` runs `git ls-remote`
- **THEN** it runs under the same rules as every other git command, the same SSH command and the 10-minute kill included

#### Scenario: Probe of the user's SSH setting
- **WHEN** gitpm checks whether `core.sshCommand` is configured
- **THEN** `git config --get core.sshCommand` runs without a timeout

#### Scenario: Catalog download
- **WHEN** a remote catalog file is fetched
- **THEN** curl runs with `--connect-timeout 5` and `--max-time 20`

#### Scenario: Credential prompt
- **WHEN** a clone of an HTTPS URL would ask for a password, also through `GIT_ASKPASS`, `SSH_ASKPASS`-style helpers or an interactive credential helper
- **THEN** git fails fast with `git` instead of waiting, so the workspace lock is released

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from one internal module, the planned switch point to embedded implementations; no other library module SHALL start processes. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any gitpm command
- **THEN** the CLI sets `Options.custom_ssh` (an FFI host sets `ManagerOptions.customSsh` and passes it to `authCheck`), gitpm then sets no SSH command of its own, and git uses the inherited variable

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

