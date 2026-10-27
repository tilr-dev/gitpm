# Release from release branches, integrate on main

## Why
The `dev` branch is gone: `main` is the only integration branch, and releases are cut as `release/X.Y` from a commit of `main` on the owner's instruction. The release process specification, release-please and the spec-change diagnostics still describe and act on the old `dev`/`main` flow: release-please would release from every push to `main`, Dependabot would target a branch that no longer exists, and a conflicting pull request would be told to merge `dev`.

## What Changes
- release-please runs only on pushes to `release/**` with the pushed branch as `target-branch`; publishing to crates.io and the binary upload follow `release_created` from that branch, as before.
- CI also runs on pull requests into and pushes to `release/**`, so the release PR and cherry-picks are checked.
- Dependabot targets the default branch `main`.
- The shared merge readiness helper tells the author to merge `main`; its digest pin in this repository's spec-change workflow is updated.
- Specifications: branch flow, CI triggers, release-please branch, `crates-io` environment branch policy, spec-change authorization and merge diagnostics refer to `main` and `release/*`; the CLI dev build example is built from `main`.

## Impact
- `.github/workflows/release.yml`, `ci.yml`, `spec-change.yml`, `.github/dependabot.yml`, `scripts/spec_merge.py` with its fixtures, tests and notes, README.
- Repositories that fetch `scripts/spec_merge.py` from `main` with a pinned digest need the new digest.
- The `crates-io` environment must allow `release/*` before the first release from a release branch (GitHub settings).
