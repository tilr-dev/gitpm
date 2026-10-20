## MODIFIED Requirements

### Requirement: Git never waits for input
Repository git commands SHALL use closed stdin, `GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS`, `credential.interactive=never`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, and the "Batch SSH command" policy. The `auth check` ls-remote and `core.sshCommand` probe SHALL be killed after 10 s (`git`, "timed out after 10s"). Verified by `exec::git_never_reads_stdin_or_prompts`.

#### Scenario: Wedged git
- **WHEN** a git subprocess hangs for 10 minutes
- **THEN** it SHALL be killed and the operation SHALL fail with `git` ("timed out after 600s")
- **AND** this is covered by `exec::wedged_subprocess_is_killed_at_the_limit`

#### Scenario: User's GIT_SSH_COMMAND
- **WHEN** the inherited environment has `GIT_SSH_COMMAND`
- **THEN** git uses it over the batch `core.sshCommand`, because git gives the variable precedence; the library does not read it

#### Scenario: Batch SSH command
- **WHEN** `Options.custom_ssh` is false and the user's Git config has no `core.sshCommand`
- **THEN** clones, fetches and `auth check` alike get `ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o StrictHostKeyChecking=accept-new`

#### Scenario: Access check ls-remote
- **WHEN** `auth check` runs `git ls-remote`
- **THEN** it has closed stdin, `GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS`, `credential.interactive=never`, the lowSpeed limits and the same batch SSH command as every other git command (not set when `Options.custom_ssh` is set or the Git config defines `core.sshCommand`), and is killed after 10 seconds with `git` ("timed out after 10s")
- **AND** this is covered by `auth::access_check_is_bounded`

#### Scenario: Probe of the user's SSH setting
- **WHEN** gitpm checks whether `core.sshCommand` is configured
- **THEN** `git -c credential.interactive=never config --get core.sshCommand` runs with empty `GIT_ASKPASS`, closed stdin, and is killed after 10 seconds (`git`, "timed out after 10s")

#### Scenario: Catalog download
- **WHEN** a remote catalog file is fetched
- **THEN** curl runs with `--connect-timeout 5` and `--max-time 20`

#### Scenario: Credential prompt
- **WHEN** a clone of an HTTPS URL would ask for a password, also through `GIT_ASKPASS`, `SSH_ASKPASS`-style helpers or an interactive credential helper
- **THEN** git fails fast with `git` instead of waiting, so the workspace lock is released

### Requirement: Read-only repository access check
`auth check SOURCE` (`auth::check`, FFI `authCheck`) SHALL verify read access with `git ls-remote` only, without cloning or opening a workspace; an empty repository SHALL count as accessible; a failing check SHALL fail with `auth` (except at the time limit, see "Silent server"), its message the diagnosis of one category (host key, SSH denied, DNS, unreachable, TLS, not found, authentication) without raw git output; host key verification SHALL never be disabled.

#### Scenario: Invalid input
- **WHEN** the source is empty, option-like, has an unsupported scheme or carries credentials
- **THEN** the check fails with `invalid_argument` before git starts; a git that cannot be started fails with `auth`

#### Scenario: Unknown host key
- **WHEN** SSH reports `Host key verification failed`
- **THEN** the check fails with `auth`, the message says to set up known_hosts with plain SSH and does not include git's stderr
- **AND** this is covered by `auth::diagnoses_failures_without_echoing_secrets`, `auth::failure_maps_every_category_and_hides_stderr`, `auth_check_is_read_only_and_accepts_empty_repositories`

#### Scenario: Silent server
- **WHEN** the server accepts the connection and sends nothing for 10 seconds
- **THEN** the check fails with `git` ("timed out after 10s"), not `auth`
- **AND** this is covered by `auth::access_check_is_bounded`
