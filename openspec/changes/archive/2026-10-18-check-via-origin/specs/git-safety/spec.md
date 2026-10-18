## ADDED Requirements

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
