# Proposal: Embedded Git and HTTP instead of system binaries

## Why

gitpm shells out to system `git` and `curl`, which on macOS requires the Command Line Tools; the repair plan listed "later: embedded git/HTTP (gix, ureq) instead of system tools, so it works on a Mac without Command Line Tools" (Basic Memory "Tilr PM — use cases и план починки", phase 5). `exec.rs` is documented as "the place to switch to gix/ureq later". Status: verified not implemented (no gix/ureq dependency). Kept in the backlog by the owner on 2026-10-18 (decision D17; tracker TILR-20).

## What Changes

- Implement the `exec` boundary (clone, fetch, rev-parse, archive, ls-remote, status, commit/tag for authoring; HTTPS GET) with embedded libraries, keeping the observable behavior of the specs (noninteractive, bounded, HTTPS-only, size limit, credentials from the user's existing setup).
- Possibly behind a cargo feature first.
- Order: after `catalog-streaming-size-limit` (TILR-12), which rewrites `exec::http_get` first; this change then replaces that function without changing the streaming limit it introduced.

## Capabilities

### Modified Capabilities
- `git-safety`: "One subprocess boundary" (delta written: Git/HTTP through one module, system tools or embedded). Planned deltas, to be written once the backend is assessed (task 0.1), because their text pins system-tool details: "Git never waits for input" (the batch SSH command string `ssh -o BatchMode=yes …`, `core.sshCommand` probe and its no-timeout exception, `GIT_TERMINAL_PROMPT`, `http.lowSpeed*`, the 600 s kill), its scenarios "Access check ls-remote", "Probe of the user's SSH setting", "Catalog download" (curl `--connect-timeout`/`--max-time`); "No option injection" (`rev-parse --end-of-options`, `--` before positionals); "Untracked files are detected regardless of git config" (`--untracked-files=all`); "URL rewrites do not hide clones" (`url.*.insteadOf`); "Read-only repository access check" (`git ls-remote`).
- `catalog`: "Catalog locations and transport" (curl flags behind "no redirects" and the size limit), planned.

## Out of scope

- Changing any CLI/API behavior.
- SSH agent/credential-helper parity beyond what gix supports (to be assessed).

## Impact

`crates/gitpm/src/exec.rs` and its callers' expectations, new dependencies (cargo-deny allowlist, MSRV 1.92 check), binary size. Tracker: TILR-20.
