# Restore security checks on dev

## Why
The history scan flags a password created solely for the loopback Git test server.
RustSec also reports the unmaintained basic-toml parser in UniFFI's build-time
Askama dependency tree. Both findings block unrelated work.

## What Changes
- Ignore only the confirmed test fixture's gitleaks fingerprint.
- Ignore only RUSTSEC-2026-0374 while released upstream dependencies still need
  basic-toml; document the evidence and removal condition.
- Keep all other secret and dependency checks enabled.

## Impact
Only security-check configuration and release-process documentation change.
Product behavior and test credentials remain unchanged.
