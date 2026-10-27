# Proposal: Rename to gitpm, decouple from Tilr, publish on crates.io

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

The owner: "a plugin is just a folder", the manager should be universal and not bound to Tilr, and the crate name on crates.io is forever (Claude S4). Positioning: an embeddable package manager where packages are folders in Git repositories. Releases had to build and publish on merge without long-lived tokens (Basic Memory note "gitpm — крейт на crates.io и стандарт GitHub-организации tilr-dev").

## What Changes

- **BREAKING** crate `tilr-plugin-manager` → `gitpm` (crates/gitpm); FFI → `gitpm-ffi` (Swift module `Gitpm`, C `GitpmFFI`, Kotlin `gitpm`, `GitpmFFI.xcframework`), `publish = false`.
- `plugin.json` keeps unknown keys in order; library messages in English without CLI flags.
- CI: fmt+clippy, test, MSRV 1.92, cargo-deny, gitleaks; Dependabot; MIT OR Apache-2.0.
- release-please with the org GitHub App `tilr-bot`; crates.io Trusted Publishing (environment `crates-io`); 0.1.0 published by hand, 0.1.1 as the first automated release.
- GitHub org standard: `main` default branch, squash-only, rulesets with required checks, protected `v*` tags.

## Capabilities

### Modified Capabilities
- `package-manifest`
- `ffi-bindings`
- `release-process`

## Out of scope

- Configurable layout, tag scheme, scaffold templates, plugin→package API rename (deferred to 0.2+; still not done).
- Distributing FFI binaries (SwiftPM/AAR) until a second consumer exists.

## Impact

Implemented by: [`0c9caf9`](https://github.com/tilr-dev/gitpm/commit/0c9caf9) feat!: rename to gitpm and prepare crates.io release (2026-10-16 17:32); [`d7dd2d8`](https://github.com/tilr-dev/gitpm/commit/d7dd2d8) fix(deps): bump sha2 to 0.11 ([#1](https://github.com/tilr-dev/gitpm/pull/1)); [`d3d7473`](https://github.com/tilr-dev/gitpm/commit/d3d7473) release 0.1.1 ([#2](https://github.com/tilr-dev/gitpm/pull/2)).
