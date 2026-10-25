## MODIFIED Requirements

### Requirement: Origin tag lookup is bounded and restricted
`check` SHALL use catalog release data when the package source is listed, without querying origin. Otherwise it SHALL validate the source, read tag names with bounded, allowlisted `ls-remote`, then fetch candidates newest first with `--depth=1 --filter=blob:none`, reading only package metadata in a disposable repository. It SHALL stop when the requested release is found, leave installed clones and workspace unchanged, and retain all transport input, timeout and SSH bounds.

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

#### Scenario: Catalog avoids origin lookup
- **WHEN** a selected catalog lists the installed package source and origin is unavailable
- **THEN** `check` reports the catalog versions and never queries origin

#### Scenario: Partial fallback skips absent packages
- **WHEN** origin tags are v3.0.0 without the installed ID, v2.0.0 with that ID and v1.0.0 older
- **THEN** fallback shallow-fetches v3.0.0 then v2.0.0 with blob filtering, reads manifests without checking out dist, and stops before v1.0.0

## REMOVED Requirements

### Requirement: Library-owned Git input
**Reason**: Repository releases create one tag with `git tag -m MESSAGE`; no multi-tag `update-ref --stdin` transaction remains.
**Migration**: Release tags use the normal non-interactive Git subprocess path, honoring `tag.gpgSign`.
