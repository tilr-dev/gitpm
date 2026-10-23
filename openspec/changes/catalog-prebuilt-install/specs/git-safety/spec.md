## MODIFIED Requirements

### Requirement: Git never waits for input
Repository git commands SHALL use closed stdin, `GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS`, `credential.interactive=never`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, and the "Batch SSH command" policy. The `auth check` ls-remote and `core.sshCommand` probe SHALL be killed after 10 s (`git`, "timed out after 10s"). Verified by `transport::system::tests::git_never_reads_stdin_or_prompts`.

#### Scenario: Wedged git
- **WHEN** a git subprocess hangs for 10 minutes
- **THEN** it SHALL be killed and the operation SHALL fail with `git` ("timed out after 600s")
- **AND** this is covered by `transport::system::tests::wedged_subprocess_is_killed_at_the_limit`

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
- **AND** a timed-out probe stops `auth check` before `ls-remote`; it cannot be treated as a configured SSH command
- **AND** this is covered by `auth_check_reports_config_probe_timeout_without_running_ls_remote`

#### Scenario: Catalog download
- **WHEN** a remote catalog file is fetched
- **THEN** curl runs with `--connect-timeout 5` and `--max-time 20`

#### Scenario: Credential prompt
- **WHEN** a clone of an HTTPS URL would ask for a password, also through `GIT_ASKPASS`, `SSH_ASKPASS`-style helpers or an interactive credential helper
- **THEN** git fails fast with `git` instead of waiting, so the workspace lock is released

#### Scenario: Bounded build request
- **WHEN** a build request stalls or streams an oversized body
- **THEN** the five-second connect timeout, twenty-second total timeout and 16 MiB streaming bound terminate it with `http`; builds alone follow at most five HTTPS-only redirects

### Requirement: Typed network transport
The transport module SHALL expose a Transport trait with typed methods for clone, fetch, remote HEAD refresh, remote ref lookup, read-only access probes, catalog HTTP GET and artifact GET. Network call sites in gitpm SHALL use these methods rather than assemble raw network Git commands. The transport API visible to production callers SHALL contain only typed operations; raw Git helpers SHALL remain internal to the transport module. Transport failures SHALL retain the existing gitpm error kinds and messages.

#### Scenario: Clone and fetch through the interface
- **WHEN** gitpm clones a repository or fetches tags, records origin tags or refreshes a remote HEAD
- **THEN** it invokes the typed transport operation with the same options, refspecs and SSH setting as before, with subprocess creation confined to the transport module

#### Scenario: Streamed catalog size bound
- **WHEN** a catalog response has no declared size
- **THEN** the transport reads at most 16 MiB plus the four-byte status trailer plus one overflow byte, kills curl on overflow, and retains the existing 200/404/error semantics
- **AND** both loopback streamed-response tests run inside the transport module

#### Scenario: Build download through the interface
- **WHEN** a package needs a build archive
- **THEN** the library invokes the typed artifact operation and subprocesses remain confined to transport

#### Scenario: Shared bounded HTTP handling
- **WHEN** catalog or build bytes are downloaded
- **THEN** artifact GET shares the bounded catalog HTTP handling while applying its build-specific HTTPS-only redirect policy
