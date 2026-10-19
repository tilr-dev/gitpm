# Proposal: One SSH policy and Options.custom_ssh

## Why

`auth check` uses its own SSH command (`StrictHostKeyChecking=yes`) while clones use the batch command, and `custom_ssh` from the environment reaches only `auth check`: a user with `GIT_SSH` passes `auth check` but installs ignore it, and an FFI host would have to copy the CLI's environment logic (review 2, G3#3, TILR-135, split from TILR-53).

## What Changes

- One batch SSH command (`StrictHostKeyChecking=accept-new`, keepalives) for clones, fetches and `auth check`.
- **BREAKING** One `Options.custom_ssh` (FFI `ManagerOptions.customSsh`) used by every git command; the CLI sets it from `GIT_SSH_COMMAND`/`GIT_SSH` for every command. The library still reads no environment.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

Shared requirement: git-safety "Git never waits for input" is also modified by `git-never-prompts-via-askpass` (TILR-53) and `bounded-auth-check` (TILR-24); each delta changes only its own part of the current text. The change archived later rebases its delta onto the spec the earlier one left. `embedded-git-and-http` keeps this change's git-safety "One subprocess boundary" and single SSH policy.

## Capabilities

### Modified Capabilities
- `git-safety`: "Git never waits for input", "One subprocess boundary".
- `cli`: "Settings come from flags, environment, defaults".
- `ffi-bindings`: new "SSH setting in ManagerOptions".

## Impact

`exec.rs` (SSH policy), `auth.rs`, CLI settings, FFI `ManagerOptions`, README rules and "Changed in 0.6", `openspec/config.yaml` context. Tracker: TILR-135. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
