# Proposal: Embedded Git instead of the system binary

## Why

gitpm shells out to system `git`, which on macOS requires the Command Line Tools; the repair plan listed "later: embedded git/HTTP (gix, ureq) instead of system tools, so it works on a Mac without Command Line Tools" (Basic Memory "Tilr PM — use cases и план починки", phase 5). An embedded implementation would also give gitpm direct control over connections, timeouts and SSH options (TILR-24, TILR-53). `exec.rs` is documented as "the place to switch to gix/ureq later". Status: verified not implemented (no gix/git2 dependency). Kept in the backlog by the owner on 2026-10-18 (decision D17; tracker TILR-20, closed as a duplicate of TILR-141).

## What Changes

- Implement the git part of the `exec` boundary (clone, fetch, rev-parse, archive, ls-remote, status, commit/tag for authoring) with an embedded library (gitoxide/gix, libgit2/git2, or a hybrid with system git as an option — decided by task 0.1), keeping the observable behavior of the specs (noninteractive, bounded, credentials from the user's existing setup).
- Possibly behind a cargo feature first.

Depends on: `one-ssh-policy-custom-ssh` (TILR-135), whose git-safety "One subprocess boundary" and single SSH policy this change keeps. Order in the tracker: after TILR-12 (`catalog-streaming-size-limit`, same `exec` layer) and TILR-146 (transport behind one interface). Research: task 0.1 decides whether it proceeds.

Shared requirement: git-safety "One subprocess boundary" is also modified by `embedded-http` (TILR-145); each delta changes only its own part (this one: Git access). The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `git-safety`: "One subprocess boundary" (delta written: Git through one module, system `git` or embedded). Planned deltas, to be written once the backend is assessed (task 0.1), because their text pins system-git details: "Git never waits for input" (the batch SSH command string `ssh -o BatchMode=yes …`, `core.sshCommand` probe and its no-timeout exception, `GIT_TERMINAL_PROMPT`, `http.lowSpeed*`, the 600 s kill), its scenarios "Access check ls-remote", "Probe of the user's SSH setting"; "No option injection" (`rev-parse --end-of-options`, `--` before positionals); "Untracked files are detected regardless of git config" (`--untracked-files=all`); "URL rewrites do not hide clones" (`url.*.insteadOf`); "Read-only repository access check" (`git ls-remote`).

## Out of scope

- Changing any CLI/API behavior.
- HTTP downloads (curl → ureq): `embedded-http` (TILR-145).
- SSH agent/credential-helper parity beyond what the chosen library supports (to be assessed).

## Impact

`crates/gitpm/src/exec.rs` (git calls) and its callers' expectations, new dependencies (cargo-deny allowlist, MSRV 1.92 check), binary size. Tracker: TILR-141 (TILR-20 closed as its duplicate). Split from the former shared change `embedded-git-and-http` (2026-10-20: one task, one change).
