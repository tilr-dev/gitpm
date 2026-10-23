# git-safety Specification

## Purpose

Defines how gitpm talks to system `git` and `curl` safely: one subprocess boundary, noninteractive and bounded git, protection against option injection, how credentials are (not) handled, the read-only access check, and hermetic `cargo test` runs.

## Requirements

### Requirement: One subprocess boundary
All subprocesses of the library (system `git` and `curl`) SHALL be started from the internal transport module, the planned switch point to embedded implementations; no other gitpm library module SHALL start processes. Single source for the library's I/O rule: the library SHALL NOT print, prompt, read stdin or read environment variables itself; a setting that depends on the environment is passed in by the caller. Verified by `no_env::garbage_pm_environment_is_ignored`.

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

#### Scenario: Git exits before reading the input
- **WHEN** the Git process closes its input before the library has written it
- **THEN** the command fails with `git` ("cannot write git input") and no SIGPIPE reaches the host process, which may keep the default SIGPIPE action (FFI)
- **AND** verified by `transport::system::tests::input_to_a_child_that_closed_stdin_fails_without_sigpipe`

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
`auth check SOURCE` (`auth::check`, FFI `authCheck`) SHALL verify read access with `git ls-remote` only, without cloning or opening a workspace; an empty repository SHALL count as accessible; a failing check SHALL fail with `auth` (except at the time limit, see "Silent server"), its message the diagnosis of one category (host key, SSH denied, DNS, unreachable, TLS, not found, authentication) without raw git output; host key verification SHALL never be disabled.

#### Scenario: Invalid input
- **WHEN** the source is empty, option-like, has an unsupported scheme or carries credentials
- **THEN** the check fails with `invalid_argument` before git starts; a git that cannot be started fails with `auth`

#### Scenario: Unknown host key
- **WHEN** SSH reports `Host key verification failed` (a changed key; a new host's key is accepted and recorded in `known_hosts` by `StrictHostKeyChecking=accept-new`)
- **THEN** the check fails with `auth`, the message says to set up known_hosts with plain SSH and does not include git's stderr
- **AND** this is covered by `auth::diagnoses_failures_without_echoing_secrets`, `auth::failure_maps_every_category_and_hides_stderr`, `auth_check_is_read_only_and_accepts_empty_repositories`

#### Scenario: Silent server
- **WHEN** the server accepts the connection and sends nothing for 10 seconds
- **THEN** the check fails with `git` ("timed out after 10s"), not `auth`
- **AND** this is covered by `auth::access_check_is_bounded`

### Requirement: Untracked files are detected regardless of git config
Local-work detection SHALL see untracked and ignored files whatever the repository's `status.showUntrackedFiles` setting (every status listing passes `--untracked-files=all`), and SHALL bound the listing of unpublished commits. Verified by `lost_work::changed_paths_sees_untracked_files_whatever_the_status_config_says` and `lost_work::unpublished_commit_listing_is_bounded`.

#### Scenario: Hidden untracked file
- **WHEN** the clone sets `status.showUntrackedFiles=no` and has an untracked file
- **THEN** local-work detection still reports it as an uncommitted change
- **AND** uninstall planning uses the same status listing

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

### Requirement: Local source clones use Git transport
Registration, catalog source discovery and package installation SHALL clone local sources through Git transport rather than copying their object directories. Transient server-info files disappearing while the source is read SHALL NOT prevent reading committed releases. Clones SHALL have their own objects and retain the original source as origin.

#### Scenario: Transient server-info file disappears
- **GIVEN** a local source with a committed tagged release and a transient `objects/info/packs_*` file removed when a filesystem copier opens it
- **WHEN** the source is registered, discovered by catalog update, or installed
- **THEN** the operation succeeds with the committed release and does not copy the transient file
- **AND** installation records the dependency edges as usual

### Requirement: SSH settings for standalone operations
Standalone catalog and check operations, which reach remotes, SHALL offer `*_with_options` entry points accepting `&Options` and use the caller's SSH policy for every Git command. Local-only operations (authoring, package and release discovery) SHALL NOT offer `*_with_options` entry points. Existing entry points SHALL use default options. Verified by `custom_ssh_is_shared_by_auth_install_catalog_and_check`.

#### Scenario: Explicit options for standalone operations
- **WHEN** a caller invokes a standalone catalog or check `*_with_options` operation with `custom_ssh: true`
- **THEN** every Git command uses the same SSH override policy as a Manager opened with those options, without reading environment variables

#### Scenario: Default standalone options
- **WHEN** an existing standalone entry point is used
- **THEN** its SSH behavior is equivalent to its `*_with_options` variant with `Options::default()`

### Requirement: Typed network transport
The transport module SHALL expose a Transport trait with typed methods for clone, fetch, remote HEAD refresh, remote ref lookup, read-only access probes and HTTP GET. Network call sites in gitpm SHALL use these methods rather than assemble raw network Git commands. The transport API visible to production callers SHALL contain only typed operations; raw Git helpers SHALL remain internal to the transport module. Transport failures SHALL retain the existing gitpm error kinds and messages.

#### Scenario: Clone and fetch through the interface
- **WHEN** gitpm clones a repository or fetches tags, records origin tags or refreshes a remote HEAD
- **THEN** it invokes the typed transport operation with the same options, refspecs and SSH setting as before, with subprocess creation confined to the transport module

#### Scenario: Streamed catalog size bound
- **WHEN** a catalog response has no declared size
- **THEN** the transport reads at most 16 MiB plus the four-byte status trailer plus one overflow byte, kills curl on overflow, and retains the existing 200/404/error semantics
- **AND** both loopback streamed-response tests run inside the transport module

### Requirement: Typed release and catalog repository operations
Release discovery, lost-work inspection, catalog generation, checks,
revision resolution, package discovery and authentication support SHALL access
Git through typed transport operations without assembling Git command arguments
in their production callers. Existing results, error kinds and messages,
repository mutations, SSH settings and execution bounds SHALL be preserved.

#### Scenario: Release inspection
- **WHEN** releases are inspected through the typed transport
- **THEN** tags, revision resolution and committed file
  reads have the same results and failure behavior as before

#### Scenario: Catalog and package content
- **WHEN** a catalog or package discovery reads committed trees and manifests
- **THEN** the same paths, raw tree entries and manifest text are returned,
  with Git argument construction confined to the transport

#### Scenario: Local work and origin inspection
- **WHEN** checks inspect repository status, refs, history or origin settings
- **THEN** the same local-work and source information is returned with the same
  SSH policy and errors through typed operations

### Requirement: Typed workspace Git operations
Production workspace management, including shared clones and forks, SHALL perform Git operations through typed methods of the internal transport trait. These callers SHALL NOT construct Git command argument lists or invoke raw Git or HTTP transport helpers. Existing behavior, output interpretation, SSH policy and error handling SHALL be preserved. Test fixture setup is outside this requirement.

#### Scenario: Workspace revisions and shared clones
- **WHEN** a workspace installs, updates, restores or removes packages and maintains shared clone refs
- **THEN** object queries, ancestry checks, checkout and ref maintenance use typed transport operations
- **AND** revision selection, retained refs and transaction behavior remain unchanged

#### Scenario: Fork an installed source
- **WHEN** installed packages are forked into a new repository
- **THEN** fetching missing revisions, selecting trees, configuring submodules, creating a branch and committing identities use typed transport operations
- **AND** the existing branch naming, history checks, hook policy and fork contents are preserved

#### Scenario: Export committed package files
- **WHEN** workspace management exports a package folder or its distribution tree
- **THEN** typed transport operations read object types, tree entries and blob bytes
- **AND** byte contents, executable permissions and existing symlink and submodule handling are preserved

### Requirement: Typed scaffold repository operations
Scaffold authoring SHALL use typed transport operations without assembling Git arguments. Operations SHALL preserve results, errors, SSH settings, execution bounds, identity, hooks, signing and rollback behavior. All production library callers SHALL use transport facades selecting the sole system implementation in one place. Test fixtures, unit and integration tests, and build scripts are outside this production-only boundary.

#### Scenario: Initialize and release a package
- **WHEN** an author initializes a repository or commits and tags a package version
- **THEN** typed operations preserve the initial branch, local demo identity fallback, staged paths, commit messages and tag behavior
- **AND** failed commits restore manifests and the index as before

#### Scenario: Release selected packages
- **WHEN** an author releases only selected package folders or releases multiple packages together
- **THEN** the selected-path commit behavior is preserved and multiple tags are created atomically without overwriting existing tags

#### Scenario: Configure origin without network access
- **WHEN** an author sets a repository's origin
- **THEN** typed operations preserve replacement of all fetch URLs and any configured push URLs without contacting the remote

#### Scenario: Replace the transport implementation
- **WHEN** the transport backend is replaced
- **THEN** one selection point inside the transport module controls all production callers
- **AND** the production boundary test rejects raw Git helpers and direct system backend access outside the transport
