# git-safety Specification

## Purpose

Defines how gitpm talks to system `git` and `curl` safely: one subprocess boundary, noninteractive and bounded git, protection against option injection, how credentials are (not) handled, the read-only access check, and hermetic `cargo test` runs.

## Requirements

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from one internal module, the planned switch point to embedded implementations; no other library module SHALL start processes. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: SSH setting from the environment
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs `gitpm auth check URL`
- **THEN** the CLI passes `custom_ssh` to the library (an FFI host passes it to `authCheck`); git subprocesses inherit the process environment, so git itself still honours such variables

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

### Requirement: Git never waits for input
Every git command on a repository (`git -C REPO …`) SHALL run with closed stdin, `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, and a batch SSH command (`BatchMode=yes`, `ConnectTimeout=10`, keepalives) passed as `core.sshCommand` unless the user's Git config defines `core.sshCommand`; one still running after 10 minutes SHALL be killed and reported as `git` ("timed out after 600s"). Verified by `exec::git_never_reads_stdin_or_prompts`.

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

### Requirement: No option injection
Single source for option-like input: ID components starting with `-` SHALL be refused with `invalid_id` before any git call, and user refs starting with `-` with `invalid_argument` ("invalid git ref") before they reach git; user refs (install, update, register) SHALL be resolved with `rev-parse --verify --end-of-options`, and positional names (sources, tags, paths) SHALL be passed after `--`. Verified by `identity::valid_part_accepts_1_to_80_safe_ascii_characters`.

#### Scenario: Option-like install ref
- **WHEN** the user runs `install SOURCE --ref --upload-pack=x`
- **THEN** the source is cloned first, then the ref is refused with `invalid_argument` before any git command sees it, and the workspace is unchanged
- **AND** this is covered by `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`

#### Scenario: Malicious source name
- **WHEN** a source or tag value is `--upload-pack=touch /tmp/x`
- **THEN** it is refused or passed after `--`, and no option is interpreted by git

### Requirement: Credentials in URLs are the user's responsibility
gitpm SHALL rely on the user's Git credentials (SSH keys, agents, credential helpers) and SHALL NOT ask for, cache or write credentials. A source URL SHALL be used as given: credentials in it are passed to git, recorded and printed as they are; keeping secrets out of URLs is the user's job. Only `auth check` inspects URLs and refuses credentials in them (scenario "Token in URL"). Verified by `auth::rejects_embedded_credentials_before_starting_git`.

#### Scenario: Credential URL given to install
- **WHEN** the user installs `https://user:token@host/repo.git`
- **THEN** the URL is not inspected, is recorded verbatim as `source` in `state.json` (a local path is recorded canonicalized) and appears as given in the `installed …` line
- **AND** git keeps the URL in the clone's `origin`

#### Scenario: Token in URL
- **WHEN** the user runs `gitpm auth check https://user:token@host/repo.git`
- **THEN** it fails with `invalid_argument` advising a credential helper, and git is not started
- **AND** a user name in an HTTP(S) URL or a password in an SSH URL (`ssh://user:pw@host/repo.git`) is refused the same way

#### Scenario: Query or fragment in an HTTP URL
- **WHEN** the user runs `gitpm auth check` with an HTTP(S) URL that has a query string or a fragment, for example `https://host/repo.git?token=X`
- **THEN** it fails with `invalid_argument` like "Token in URL", git is not started and the value is not printed

### Requirement: Read-only repository access check
`auth check SOURCE` (`auth::check`, FFI `authCheck`) SHALL verify read access with `git ls-remote` only, without cloning or opening a workspace; an empty repository SHALL count as accessible; a failing check SHALL fail with `auth`, its message the diagnosis of one category (host key, SSH denied, DNS, unreachable, TLS, not found, authentication) without echoing raw git output, and host key verification SHALL never be disabled.

#### Scenario: Invalid input
- **WHEN** the source is empty, option-like, has an unsupported scheme or carries credentials
- **THEN** the check fails with `invalid_argument` before git starts; a git that cannot be started fails with `auth`

#### Scenario: Unknown host key
- **WHEN** SSH reports `Host key verification failed`
- **THEN** the check fails with `auth`, the message says to set up known_hosts with plain SSH and does not include git's stderr
- **AND** this is covered by `auth::diagnoses_failures_without_echoing_secrets`, `auth::failure_maps_every_category_and_hides_stderr`, `auth_check_is_read_only_and_accepts_empty_repositories`

### Requirement: Untracked files are detected regardless of git config
Local-work detection SHALL see untracked and ignored files whatever the repository's `status.showUntrackedFiles` setting (every status listing passes `--untracked-files=all`), and SHALL bound the listing of unpublished commits. Verified by `fork::dirty_sees_untracked_files_whatever_the_status_config_says` (the check before adding packages to an existing fork) and `lost_work::unpublished_commit_listing_is_bounded`.

#### Scenario: Hidden untracked file
- **WHEN** the clone sets `status.showUntrackedFiles=no` and has an untracked file
- **THEN** the check before adding packages to an existing fork still reports it as an uncommitted change
- **AND** uninstall planning uses the same status listing but has no test with that setting

### Requirement: URL rewrites do not hide clones
Origin checks of shared clones SHALL compare sources so that a `url.*.insteadOf` rewrite in the user's Git config does not hide an existing clone. Verified by `a_url_rewrite_in_the_git_config_does_not_hide_the_shared_clone`.

#### Scenario: insteadOf
- **WHEN** the user's Git config rewrites the source URL and a second package of the same source is installed
- **THEN** it reuses the existing shared clone

### Requirement: Hermetic tests
The Rust tests run by `cargo test` SHALL NOT inherit the developer's `GITPM_*`/`PM_*` variables, `HOME` or Git config, and SHALL NOT use the network: `.cargo/config.toml` SHALL pin `GIT_CONFIG_GLOBAL` to `tests/gitconfig` and set `GIT_CONFIG_NOSYSTEM=1`, and CLI tests SHALL run the binary with a cleared environment (`env_clear`) and set their own `HOME`, `TMPDIR`, Git config and `GITPM_*` values. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: Dagger suite
- **WHEN** the developer runs `./itests/dt` (integration-tests)
- **THEN** it reads its own host variables (for example `DT_ENGINE`) and is outside this rule

#### Scenario: Test-only variables
- **WHEN** `cargo test --workspace` runs
- **THEN** the only variables tests read themselves are `GITPM_KEEP_TEST_ARTIFACTS` (keep fixture folders) and `GITPM_TEST_HOLD_LOCK` (lock helper process)

#### Scenario: Developer git config
- **WHEN** the developer's global Git config enables commit signing
- **THEN** `cargo test` in this repository is unaffected

### Requirement: Origin tag lookup is bounded and restricted
The repository-less `git ls-remote --tags -- SOURCE` of `check` SHALL run in `exec.rs` with the bounds of every repository command plus `LC_ALL=C`, the OS temp dir as working directory and a transport allowlist (file, git, http, https, ssh), and only on a source that passes `auth::validate_recorded`; nothing is fetched or written. Verified by `exec::ls_remote_tags_lists_tags_with_peeled_commits_and_fails_as_git`.

#### Scenario: Bounds
- **WHEN** `check` reads an origin's tags
- **THEN** git runs with closed stdin, `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, the batch `core.sshCommand` unless the user configured one, the 10-minute kill, `--` before the source, `LC_ALL=C`, the temp dir as cwd (a bare word is never a remote of the caller's repository) and `protocol.allow=never` with `protocol.{file,git,http,https,ssh}.allow=always`

#### Scenario: Source rule
- **WHEN** the source is not a Git URL (`https`, `http`, `ssh`, `git`, `file`, `host:path`, no credentials) or an absolute path, or contains `::`
- **THEN** git is not started; this is covered by `auth::tests::recorded_sources_are_urls_or_absolute_paths_without_helpers`

#### Scenario: Remote helper from a foreign manifest
- **WHEN** an installed fork's `forkOf.source` (or a recorded source) is `ext::sh -c touch% /tmp/pwned`
- **THEN** `check` does not start git for it, nothing is executed, and the row (or upstream) carries `problem` `source not queried: …`
- **AND** even an unvalidated call to the lookup is refused by git (`transport 'ext' not allowed`)

#### Scenario: Local paths stay allowed
- **WHEN** the recorded source is an absolute local path (Git 2.38.1 and later default `protocol.file.allow` to `user`)
- **THEN** the lookup still reads its tags, because the allowlist sets `protocol.file.allow=always` instead of `GIT_PROTOCOL_FROM_USER=0`
