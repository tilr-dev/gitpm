# Proposal: Robustness fixes from spec review 2

## Why

Review 2 (2026-10-18) found defects where the code misses a contract the specs state or should state; each has a tracker task: active packages below a blocked `dist/<ns>` path are left out of `DistSync.skipped`, so `sync-dist` exits 0 claiming "already holds exactly" (G1#18, TILR-51); a new `state.json` is written with a plain write, and a folder holding only `.DS_Store` is refused as non-empty (G1#24, TILR-52); `GIT_TERMINAL_PROMPT=0` does not stop `GIT_ASKPASS` or GUI credential helpers, `auth check` and clones use different SSH policies, and `GIT_SSH` passes `auth check` but is overridden for installs (G1#13, G3#3, TILR-53); transactions never fsync (G1#5, TILR-54); the release step of `fork --replace` is lenient about metadata, does not restore the index and may leave partial tags, unlike `commit` (G2#10, TILR-57); the identity check accepts an identity git derives from login and host name and fails with `git` (G2#13, TILR-49). `git archive` honours `export-ignore`, so deployed files can differ from hashed ones (G1#15, TILR-46; the contract gets a scenario here).

## What Changes

- `sync-dist` and every dist report list active packages below a blocked namespace path in `skipped` (then `not_deployed`).
- New `state.json` written via temp file + rename; a folder with only `.DS_Store` counts as empty.
- Every git command, `auth check` included: empty `GIT_ASKPASS`, `credential.interactive=never`, one batch SSH command (`StrictHostKeyChecking=accept-new`, keepalives) and the 10-minute kill; **BREAKING** one `Options.custom_ssh` (FFI `ManagerOptions.customSsh`) set by the CLI from `GIT_SSH_COMMAND`/`GIT_SSH` for all commands.
- Commit fsyncs the journal, `state.json` and parent folders; "crash" = process death or power loss.
- `fork --replace` release step behaves like `commit` (strict metadata, restore on failure, all tags or none).
- **BREAKING** Identity check with `user.useConfigOnly`; missing identity → `invalid_argument`.
- Deploys and hashes ignore Git attributes (`export-ignore`, `export-subst`).

Depends on: all earlier changes of the batch (1 → 2 → 3 → 6 → 7 → 8 → 11 → 5 → 4 → 9 → 10). Rebase: `embedded-git-and-http` modifies git-safety "One subprocess boundary".

## Capabilities

### Modified Capabilities
- `dist`, `workspace-layout`, `git-safety`, `transactions-locking`, `authoring`, `multi-package-repos`, `fork-replacement`, `plugin-metadata`, `cli`, `ffi-bindings`.

## Out of scope

- TILR-45 (lock released by a refused second open) and TILR-50 (messages without ID): their contracts are already in the specs.
- Embedded git/HTTP (`embedded-git-and-http`).

## Impact

`workspace.rs` (dist report, `open`), `txn.rs` (fsync), `exec.rs` (environment, SSH policy), `auth.rs`, `workspace/fork.rs` (release step), `scaffold.rs` (identity), deploy/hash code paths, CLI settings, FFI `ManagerOptions`, README rules and "Changed in 0.6".
