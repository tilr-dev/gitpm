# Proposal: Robustness fixes from spec review 2

## Why

Review 2 (2026-10-18) found defects where the code misses a contract the specs state or should state; each has a tracker task: active packages below a blocked `dist/<ns>` path are left out of `DistSync.skipped`, so `sync-dist` exits 0 claiming "already holds exactly" (G1#18, TILR-51); a new `state.json` is written with a plain write, and a folder holding only `.DS_Store` is refused as non-empty (G1#24, TILR-52); `GIT_TERMINAL_PROMPT=0` does not stop `GIT_ASKPASS` or GUI credential helpers, `auth check` and clones use different SSH policies, and `GIT_SSH` passes `auth check` but is overridden for installs (G1#13, G3#3, TILR-53); transactions never fsync (G1#5, TILR-54); the release step of `fork --replace` is lenient about metadata, does not restore the index and may leave partial tags, unlike `commit` (G2#10, TILR-57); the identity check accepts an identity git derives from login and host name and fails with `git` (G2#13, TILR-49). `git archive` honours `export-ignore`, so deployed files can differ from hashed ones (G1#15, TILR-46; the contract gets a scenario here). `try_lock` opens the lock file before it checks the in-process set of held locks, so a refused second `Manager::open` in the same process drops its file descriptor and thereby releases the first Manager's POSIX record lock; the same holds for the catalog lock (TILR-45). Several refusals do not name the package although spec `errors` "Errors name the package" requires it: "upstream changed without a version bump", "upstream history diverged…", "rollback manifest mismatch", "plugin id changed upstream" and the unpublished-local-work refusal of `update all` (TILR-50; the moved-tag and retracted-release messages are gone with `no-tag-immutability`). `auth check` runs `git ls-remote` without a timeout and with an SSH command without keepalives, and the `git config --get core.sshCommand` probe has no timeout (TILR-24). A Git submodule inside `dist/` deploys as an empty folder instead of being refused like a symlink (TILR-26).

## What Changes

- `sync-dist` and every dist report list active packages below a blocked namespace path in `skipped` (then `not_deployed`).
- New `state.json` written via temp file + rename; a folder with only `.DS_Store` counts as empty.
- Every git command, `auth check` included: empty `GIT_ASKPASS`, `credential.interactive=never`, one batch SSH command (`StrictHostKeyChecking=accept-new`, keepalives) and the 10-minute kill; **BREAKING** one `Options.custom_ssh` (FFI `ManagerOptions.customSsh`) set by the CLI from `GIT_SSH_COMMAND`/`GIT_SSH` for all commands.
- Commit fsyncs the journal, `state.json` and parent folders; "crash" = process death or power loss.
- `fork --replace` release step behaves like `commit` (strict metadata, restore on failure, all tags or none).
- **BREAKING** Identity check with `user.useConfigOnly`; missing identity → `invalid_argument`.
- Deploys and hashes ignore Git attributes (`export-ignore`, `export-subst`).
- Lock attempts check the in-process set of held locks (canonical path) before opening the lock file, so a refused second open never releases a held workspace or catalog lock (TILR-45).
- Every library error about a package names its ID; the CLI adds nothing; FFI `PmError` carries `id` (TILR-50).
- `auth check` and the `core.sshCommand` probe get the 10-minute kill and the batch SSH options of every other git call (TILR-24).
- **BREAKING** A submodule inside a package dist is refused with `unsupported` like a symlink (TILR-26).

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`, `removal-and-restore-rules`, `no-tag-immutability`, `protected-catalog`, `pre-release-versions`, `host-compatibility` (last change of the chain; 0.6.0 is released after it). `embedded-git-and-http` builds on its git-safety "One subprocess boundary".

## Capabilities

### Modified Capabilities
- `dist` ("Unknown paths are left alone", "Deploys copy the committed tree": submodules refused), `workspace-layout`, `git-safety` ("Git never waits for input", "One subprocess boundary"), `transactions-locking` ("Crash recovery", "Locks are process-owned and survive crashes"), `errors` ("Errors name the package"), `authoring`, `multi-package-repos`, `fork-replacement`, `plugin-metadata`, `cli`, `ffi-bindings` ("Errors keep kind and message"; new "SSH setting in ManagerOptions").

## Out of scope

- Embedded git/HTTP (`embedded-git-and-http`).

## Impact

`workspace.rs` (dist report, `open`), `txn.rs` (fsync), `exec.rs` (environment, SSH policy), `auth.rs`, `workspace/fork.rs` (release step), `scaffold.rs` (identity), deploy/hash code paths (submodules), `lock.rs` (in-process check first), update/rollback messages, CLI settings, FFI `ManagerOptions`, README rules and "Changed in 0.6". Tracker: TILR-51, TILR-52, TILR-53, TILR-54, TILR-57, TILR-49, TILR-46, TILR-45, TILR-50, TILR-24, TILR-26.
