# Proposal: Embedded HTTP client instead of curl

## Why

gitpm downloads over HTTPS (catalog reads) by shelling out to system `curl`; the repair plan listed "later: embedded git/HTTP (gix, ureq) instead of system tools, so it works on a Mac without Command Line Tools" (Basic Memory "Tilr PM — use cases и план починки", phase 5). `exec.rs` is documented as "the place to switch to gix/ureq later". Status: verified not implemented (no ureq dependency). Split out of TILR-20 into TILR-145; on pause (Dan, 2026-10-19: not needed yet).

## What Changes

- Implement the HTTP part of the `exec` boundary (`exec::http_get`, HTTPS GET) with ureq, keeping the observable behavior of the specs (bounded, HTTPS-only, no redirects, size limit).
- Possibly behind a cargo feature first.
- Order: after `catalog-streaming-size-limit` (TILR-12), which rewrites `exec::http_get` first; this change then replaces that function without changing the streaming limit it introduced.

Depends on: `catalog-streaming-size-limit` and `one-ssh-policy-custom-ssh` (TILR-135), whose git-safety "One subprocess boundary" this change keeps. Order in the tracker: after TILR-146 (transport behind one interface). Research: task 0.1 decides whether it proceeds.

Shared requirement: git-safety "One subprocess boundary" is also modified by `embedded-git` (TILR-141); each delta changes only its own part (this one: HTTP access). The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `git-safety`: "One subprocess boundary" (delta written: HTTP through one module, system `curl` or embedded). Planned delta, to be written once the client is assessed (task 0.1), because its text pins curl details: "Git never waits for input", scenario "Catalog download" (curl `--connect-timeout`/`--max-time`).
- `catalog`: "Catalog locations and transport" (curl flags behind "no redirects" and the size limit), planned.

## Out of scope

- Changing any CLI/API behavior.
- Git operations (system git → embedded): `embedded-git` (TILR-141).

## Impact

`crates/gitpm/src/exec.rs` (`http_get`) and its callers' expectations, new dependencies (cargo-deny allowlist, MSRV 1.92 check), binary size. Tracker: TILR-145. Split from the former shared change `embedded-git-and-http` (2026-10-20: one task, one change).
