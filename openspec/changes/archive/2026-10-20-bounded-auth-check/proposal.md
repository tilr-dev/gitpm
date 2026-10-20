# Proposal: Bound auth check and the core.sshCommand probe

## Why

`auth check` runs `git ls-remote` without a time limit and with an SSH command without keepalives, so it hangs forever on a stalled connection; the `git config --get core.sshCommand` probe has no time limit either (TILR-24). Decision (Dan, 2026-10-19, in TILR-24): a short ~10-second limit for both, not 10 minutes — if access does not work, waiting longer does not help; 10 minutes stays for heavy git operations.

## What Changes

- The `auth check` ls-remote gets SSH keepalives and is killed after 10 seconds (`git`, "timed out after 10s").
- The `git config --get core.sshCommand` probe gets closed stdin and the same 10-second kill.
- Error kind: an `auth check` that hits the limit fails with `git` (before: `auth` once git gave up on its own, or no failure at all while git kept waiting); every other failing check stays `auth`.

Shared requirement: git-safety "Git never waits for input" is also modified by `git-never-prompts-via-askpass` (TILR-53) and `one-ssh-policy-custom-ssh` (TILR-135); each delta changes only its own part of the current text. The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `git-safety`: "Git never waits for input", "Read-only repository access check" (the timeout exception to the `auth` kind).

## Impact

`exec.rs` (ls-remote for `auth check`, the `core.sshCommand` probe), `auth.rs` (SSH command), `openspec/config.yaml` context. Tracker: TILR-24. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
