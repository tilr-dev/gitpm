# Proposal: Embedded Git and HTTP instead of system binaries

## Why

gitpm shells out to system `git` and `curl`, which on macOS requires the Command Line Tools; the repair plan listed "later: embedded git/HTTP (gix, ureq) instead of system tools, so it works on a Mac without Command Line Tools" (Basic Memory "Tilr PM — use cases и план починки", phase 5). `exec.rs` is documented as "the place to switch to gix/ureq later". Status: verified not implemented (no gix/ureq dependency).

## What Changes

- Implement the `exec` boundary (clone, fetch, rev-parse, archive, ls-remote, status, commit/tag for authoring; HTTPS GET) with embedded libraries, keeping the observable behavior of the specs (noninteractive, bounded, HTTPS-only, size limit, credentials from the user's existing setup).
- Possibly behind a cargo feature first.

## Capabilities

### Modified Capabilities
- None intended: the specs describe behavior that must stay identical (`git-safety` wording about "system git" may need a delta later). `skip_specs: true`.

## Out of scope

- Changing any CLI/API behavior.
- SSH agent/credential-helper parity beyond what gix supports (to be assessed).

## Impact

`crates/gitpm/src/exec.rs` and its callers' expectations, new dependencies (cargo-deny allowlist, MSRV 1.92 check), binary size.
