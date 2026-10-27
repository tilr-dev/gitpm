# Proposal: Git never prompts through askpass or credential helpers

## Why

`GIT_TERMINAL_PROMPT=0` does not stop `GIT_ASKPASS`/`core.askPass` or a GUI credential helper, so a GUI host (Tilr) can wait up to 600 s for input while holding the workspace lock (review 2, G1#13, TILR-53).

## What Changes

- Every git command, `auth check` included, runs with an empty `GIT_ASKPASS` and `-c credential.interactive=never` (git ≥ 2.46).

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

Shared requirement: git-safety "Git never waits for input" is also modified by `bounded-auth-check` (TILR-24) and `one-ssh-policy-custom-ssh` (TILR-135); each delta changes only its own part of the current text. The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `git-safety`: "Git never waits for input".

## Impact

`exec.rs` (environment and args of every git call). Tracker: TILR-53. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
