## MODIFIED Requirements

### Requirement: Git never waits for input
Repository git commands SHALL use closed stdin (or library-owned input closed after writing), `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20` and batch SSH via `core.sshCommand` unless user-defined. User stdin SHALL NOT be inherited; a command running after 10 minutes SHALL be killed with `git` ("timed out after 600s").

#### Scenario: Wedged git
- **WHEN** a git subprocess hangs for 10 minutes
- **THEN** it is killed and the operation fails with `git`
- **AND** this is covered by `exec::wedged_subprocess_is_killed_at_the_limit`

#### Scenario: User's GIT_SSH_COMMAND
- **WHEN** the inherited environment has `GIT_SSH_COMMAND`
- **THEN** git uses it over the batch `core.sshCommand`, because git gives the variable precedence; the library does not read it

#### Scenario: Batch SSH command
- **WHEN** the user's Git config has no `core.sshCommand` and the environment has no `GIT_SSH_COMMAND`
- **THEN** git gets `ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4`

#### Scenario: Access check ls-remote
- **WHEN** `auth check` runs `git ls-remote`
- **THEN** it has closed stdin, `GIT_TERMINAL_PROMPT=0` and the lowSpeed limits, but its own SSH command (`BatchMode=yes`, `ConnectTimeout=10`, `StrictHostKeyChecking=yes`, no keepalives; not set when the caller passes `custom_ssh` or the Git config defines `core.sshCommand`) and no wall-clock kill

#### Scenario: Probe of the user's SSH setting
- **WHEN** gitpm checks whether `core.sshCommand` is configured
- **THEN** `git config --get core.sshCommand` runs without a timeout

#### Scenario: Catalog download
- **WHEN** a remote catalog file is fetched
- **THEN** curl runs with `--connect-timeout 5` and `--max-time 20`

#### Scenario: Credential prompt
- **WHEN** a clone of an HTTPS URL would ask for a password
- **THEN** git fails fast with `git` instead of waiting, so the workspace lock is released

## ADDED Requirements

### Requirement: Library-owned Git input
Git ref transactions SHALL accept only library-generated input, close the input pipe after writing, and retain the execution timeout and noninteractive bounds.

#### Scenario: Library-owned ref transaction input
- **WHEN** release tags are created by `update-ref --stdin`
- **THEN** only library-generated commands are supplied, followed by EOF, with the same execution timeout
- **AND** verified by `fork_replace_release_late_tag_lock_leaves_no_tags`
