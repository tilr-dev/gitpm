# git-safety Specification

## Purpose

Defines how gitpm talks to system `git` and `curl` safely: one subprocess boundary, noninteractive and bounded git, protection against option injection, credential handling, the read-only access check, and hermetic tests.

## Requirements

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from one internal module (`exec`), the planned switch point to embedded implementations; no other library module SHALL start processes. The library SHALL NOT print, prompt, read stdin or read environment variables itself. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: Garbage environment
- **WHEN** `PM_*` and `GITPM_*` variables hold garbage while the library is used
- **THEN** results are the same as with a clean environment

### Requirement: Git never waits for input
Every git command on a repository (`git -C REPO …`) SHALL run with closed stdin, `GIT_TERMINAL_PROMPT=0`, `http.lowSpeedLimit=1`/`http.lowSpeedTime=20`, and a batch SSH command (`BatchMode=yes`, `ConnectTimeout=10`, keepalives) unless the user configured `core.sshCommand` or `GIT_SSH_COMMAND`; one still running after 10 minutes SHALL be killed and reported as `git` ("timed out after 600s"). Verified by `exec::git_never_reads_stdin_or_prompts` and `exec::wedged_subprocess_is_killed_at_the_limit`.

#### Scenario: Batch SSH command
- **WHEN** the user has no `core.sshCommand` and no `GIT_SSH_COMMAND`
- **THEN** git gets `ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4`

#### Scenario: Access check ls-remote
- **WHEN** `auth check` runs `git ls-remote`
- **THEN** it has closed stdin, `GIT_TERMINAL_PROMPT=0` and the lowSpeed limits, but its own SSH command (`BatchMode=yes`, `ConnectTimeout=10`, `StrictHostKeyChecking=yes`, no keepalives) and no wall-clock kill

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
ID components starting with `-` SHALL be refused before any git call, and user refs starting with `-` before they reach git; user refs (install, update, register) SHALL be resolved with `rev-parse --verify --end-of-options`, and positional names (sources, tags, paths) SHALL be passed after `--`. Verified by `identity::valid_part_accepts_1_to_80_safe_ascii_characters` and `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`.

#### Scenario: Option-like install ref
- **WHEN** the user runs `install SOURCE --ref --upload-pack=x`
- **THEN** the source is cloned first, then the ref is refused with `invalid_argument` before any git command sees it (see install-uninstall)

#### Scenario: Malicious source name
- **WHEN** a source or tag value is `--upload-pack=touch /tmp/x`
- **THEN** it is refused or passed after `--`, and no option is interpreted by git

### Requirement: Credentials are never stored or echoed
gitpm SHALL rely on the user's existing Git credentials (SSH keys, agents, credential helpers) and SHALL NOT ask for, cache or write credentials itself. Only the access check (`auth check`) inspects URLs: a user name or token in an HTTP(S) URL, or a password in an SSH URL, SHALL be refused there before git starts. Verified by `auth::rejects_embedded_credentials_before_starting_git`.

#### Scenario: Credential URL given to install
- **WHEN** the user installs `https://user:token@host/repo.git`
- **THEN** the URL is not inspected and is recorded verbatim as `source` in `state.json` (a local path is recorded canonicalized)

#### Scenario: Token in URL
- **WHEN** the user runs `gitpm auth check https://user:token@host/repo.git`
- **THEN** it fails with `invalid_argument` advising a credential helper, and git is not started

### Requirement: Read-only repository access check
`auth check SOURCE` (`auth::check`, FFI `authCheck`) SHALL verify read access with `git ls-remote` only, without cloning; an empty repository SHALL count as accessible; failures SHALL map to a diagnosis category (host key, SSH denied, DNS, unreachable, TLS, not found, authentication) without echoing raw git output, and host key verification SHALL never be disabled.

#### Scenario: Unknown host key
- **WHEN** SSH reports `Host key verification failed`
- **THEN** the result says to set up known_hosts with plain SSH and does not include git's stderr
- **AND** this is covered by `auth::diagnoses_failures_without_echoing_secrets`, `auth::failure_maps_every_category_and_hides_stderr`, `auth_check_is_read_only_and_accepts_empty_repositories`

### Requirement: Untracked files are detected regardless of git config
Local-work detection SHALL see untracked and ignored files whatever the repository's `status.showUntrackedFiles` setting (every status listing passes `--untracked-files=all`), and SHALL bound the listing of unpublished commits. Verified for the fork-add dirty check by `fork::dirty_sees_untracked_files_whatever_the_status_config_says` and by `lost_work::unpublished_commit_listing_is_bounded`; uninstall planning uses the same status listing but has no test with that setting.

#### Scenario: Hidden untracked file
- **WHEN** the clone sets `status.showUntrackedFiles=no` and has an untracked file
- **THEN** the check before adding packages to an existing fork still reports it as an uncommitted change

### Requirement: URL rewrites do not hide clones
Origin checks of shared clones SHALL compare sources so that a `url.*.insteadOf` rewrite in the user's Git config does not hide an existing clone. Verified by `a_url_rewrite_in_the_git_config_does_not_hide_the_shared_clone`.

#### Scenario: insteadOf
- **WHEN** the user's Git config rewrites the source URL and a second package of the same source is installed
- **THEN** it reuses the existing shared clone

### Requirement: Hermetic tests
Tests SHALL NOT inherit the developer's `GITPM_*`/`PM_*` variables, `HOME` or Git config, and SHALL NOT use the network: `.cargo/config.toml` SHALL pin `GIT_CONFIG_GLOBAL` to `tests/gitconfig` and set `GIT_CONFIG_NOSYSTEM=1`, and CLI tests SHALL run the binary with a cleared environment (`env_clear`) and set their own `HOME`, `TMPDIR`, Git config and `GITPM_*` values. Verified by `no_env::garbage_pm_environment_is_ignored`.

#### Scenario: Test-only variables
- **WHEN** the test suite runs
- **THEN** the only variables tests read themselves are `GITPM_KEEP_TEST_ARTIFACTS` (keep fixture folders) and `GITPM_TEST_HOLD_LOCK` (lock helper process)

#### Scenario: Developer git config
- **WHEN** the developer's global Git config enables commit signing
- **THEN** `cargo test` in this repository is unaffected
