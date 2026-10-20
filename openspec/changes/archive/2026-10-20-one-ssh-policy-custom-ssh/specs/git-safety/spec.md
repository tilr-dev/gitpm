## MODIFIED Requirements

### Requirement: Git never waits for input
Repository git commands SHALL use closed stdin, `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, and the "Batch SSH command" policy. A command still running after 10 minutes SHALL be killed and reported as `git` ("timed out after 600s"). Verified by `exec::git_never_reads_stdin_or_prompts`.

#### Scenario: Wedged git
- **WHEN** a git subprocess hangs for 10 minutes
- **THEN** it is killed and the operation fails with `git`
- **AND** this is covered by `exec::wedged_subprocess_is_killed_at_the_limit`

#### Scenario: User's GIT_SSH_COMMAND
- **WHEN** the inherited environment has `GIT_SSH_COMMAND`
- **THEN** git uses it over the batch `core.sshCommand`, because git gives the variable precedence; the library does not read it

#### Scenario: Batch SSH command
- **WHEN** `Options.custom_ssh` is false and the user's Git config has no `core.sshCommand`
- **THEN** clones, fetches and `auth check` alike get `ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o StrictHostKeyChecking=accept-new`

#### Scenario: Access check ls-remote
- **WHEN** `auth check` runs `git ls-remote`
- **THEN** it has closed stdin, `GIT_TERMINAL_PROMPT=0`, the lowSpeed limits and the same batch SSH command as every other git command (not set when `Options.custom_ssh` is set or the Git config defines `core.sshCommand`), and no wall-clock kill

#### Scenario: Probe of the user's SSH setting
- **WHEN** gitpm checks whether `core.sshCommand` is configured
- **THEN** `git config --get core.sshCommand` runs without a timeout

#### Scenario: Catalog download
- **WHEN** a remote catalog file is fetched
- **THEN** curl runs with `--connect-timeout 5` and `--max-time 20`

#### Scenario: Credential prompt
- **WHEN** a clone of an HTTPS URL would ask for a password
- **THEN** git fails fast with `git` instead of waiting, so the workspace lock is released

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from one internal module, the planned switch point to embedded implementations; no other library module SHALL start processes. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any gitpm command
- **THEN** the CLI sets `Options.custom_ssh` (an FFI host sets `ManagerOptions.customSsh` and passes it to `authCheck`), gitpm then sets no SSH command of its own, and git uses the inherited variable

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

### Requirement: Origin tag lookup is bounded and restricted
The repository-less `git ls-remote --tags --heads -- SOURCE` of `check` SHALL run in `exec.rs` with the bounds of every repository command plus `LC_ALL=C`, the OS temp dir as working directory and a transport allowlist (file, git, http, https, ssh), and only on a source that passes `auth::validate_recorded`; nothing is fetched or written. Verified by `exec::ls_remote_refs_lists_tags_with_peeled_commits_and_branches_and_fails_as_git`.

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

### Requirement: SSH settings for standalone operations
Standalone catalog, ref, check and authoring operations SHALL offer `*_with_options` entry points accepting `&Options` and use the caller's SSH policy for every Git command. Existing entry points SHALL use default options. Verified by `custom_ssh_is_shared_by_auth_install_catalog_and_check`.

#### Scenario: Explicit options for standalone operations
- **WHEN** a caller invokes a standalone catalog, ref, check or authoring `*_with_options` operation with `custom_ssh: true`
- **THEN** every Git command uses the same SSH override policy as a Manager opened with those options, without reading environment variables

#### Scenario: Default standalone options
- **WHEN** an existing standalone entry point is used
- **THEN** its SSH behavior is equivalent to its `*_with_options` variant with `Options::default()`
