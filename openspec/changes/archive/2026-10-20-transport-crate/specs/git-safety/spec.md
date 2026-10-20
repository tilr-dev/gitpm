## MODIFIED Requirements

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from the internal transport module, the planned switch point to embedded implementations; no other gitpm library module SHALL start processes. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: Process boundary enforcement
- **WHEN** library source is checked
- **THEN** production process creation is confined to the transport module, verified by `transport::boundary_tests::production_processes_stay_inside_transport`

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any gitpm command
- **THEN** the CLI sets `Options.custom_ssh` (an FFI host sets `ManagerOptions.customSsh` and passes it to `authCheck`), gitpm then sets no SSH command of its own, and git uses the inherited variable

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

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

### Requirement: Library-owned Git input
Exception to "Git never waits for input": a Git ref transaction (`update-ref --stdin`) SHALL get only library-generated input instead of closed stdin, and the input pipe SHALL be closed after writing; user stdin SHALL NOT be inherited, and the other bounds of that requirement (no prompts, lowSpeed limits, batch SSH, 600s kill) SHALL apply unchanged.

#### Scenario: Library-owned ref transaction input
- **WHEN** release tags are created by `update-ref --stdin`
- **THEN** only library-generated commands are supplied, followed by EOF, with the same execution timeout
- **AND** verified by `fork_replace_release_late_tag_lock_leaves_no_tags`

#### Scenario: Git exits before reading the input
- **WHEN** the Git process closes its input before the library has written it
- **THEN** the command fails with `git` ("cannot write git input") and no SIGPIPE reaches the host process, which may keep the default SIGPIPE action (FFI)
- **AND** verified by `transport::system::tests::input_to_a_child_that_closed_stdin_fails_without_sigpipe`

### Requirement: Origin tag lookup is bounded and restricted
The repository-less `git ls-remote --tags --heads -- SOURCE` of `check` SHALL run in the internal transport module with the bounds of every repository command plus `LC_ALL=C`, the OS temp dir as working directory and a transport allowlist (file, git, http, https, ssh), and only on a source that passes `auth::validate_recorded`; nothing is fetched or written. Verified by `transport::system::tests::ls_remote_refs_lists_tags_with_peeled_commits_and_branches_and_fails_as_git`.

#### Scenario: Bounds
- **WHEN** `check` reads an origin's tags and branches
- **THEN** git runs with closed stdin, `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, the batch `core.sshCommand` unless `Options.custom_ssh` is set or the user configured one, the 10-minute kill, `--` before the source, `LC_ALL=C`, the temp dir as cwd (a bare word is never a remote of the caller's repository) and `protocol.allow=never` with `protocol.{file,git,http,https,ssh}.allow=always`

#### Scenario: Source rule
- **WHEN** the source is not a Git URL (`https`, `http`, `ssh`, `git`, `file`, `host:path`, no credentials) or an absolute path, or contains `::`
- **THEN** git is not started; this is covered by `auth::tests::recorded_sources_are_urls_or_absolute_paths_without_helpers`

#### Scenario: Remote helper from a foreign manifest
- **WHEN** an installed fork's `forkOf.source` (or a recorded source) is `ext::sh -c touch% /tmp/pwned`
- **THEN** `check` does not start git for it, nothing is executed, and the row (or upstream) carries `problem` `source not queried: …`
- **AND** even an unvalidated call to the lookup is refused by git (`transport 'ext' not allowed`)

#### Scenario: Local paths stay allowed
- **WHEN** the recorded source is an absolute local path (Git 2.38.1 and later default `protocol.file.allow` to `user`)
- **THEN** the lookup still reads its tags and branches, because the allowlist sets `protocol.file.allow=always` instead of `GIT_PROTOCOL_FROM_USER=0`

## ADDED Requirements

### Requirement: Typed network transport
The transport module SHALL expose a Transport trait with typed methods for clone, fetch, remote HEAD refresh, remote ref lookup, read-only access probes and HTTP GET. Network call sites in gitpm SHALL use these methods rather than assemble raw network Git commands. Raw local Git commands SHALL remain available through the same boundary for subsequent migration tasks. Transport failures SHALL retain the existing gitpm error kinds and messages.

#### Scenario: Clone and fetch through the interface
- **WHEN** gitpm clones a repository or fetches tags, records origin tags or refreshes a remote HEAD
- **THEN** it invokes the typed transport operation with the same options, refspecs and SSH setting as before, with subprocess creation confined to the transport module

#### Scenario: Streamed catalog size bound
- **WHEN** a catalog response has no declared size
- **THEN** the transport reads at most 16 MiB plus the four-byte status trailer plus one overflow byte, kills curl on overflow, and retains the existing 200/404/error semantics
- **AND** both loopback streamed-response tests run inside the transport module
