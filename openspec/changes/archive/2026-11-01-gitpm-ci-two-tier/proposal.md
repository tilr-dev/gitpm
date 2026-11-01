# Two-tier CI: fast tests on pull requests, every test in the merge queue

## Why
The full test set ran up to four times for one change: locally before review, on the pull request, in the merge queue and again on the push to `main` after the merge. Pull request feedback waited for every long scenario test, and the push run re-checked a commit the merge queue had already checked.

## What Changes
- The required checks keep their names and run on every pull request, merge queue group and push to `release/**`; the push to `main` no longer triggers CI.
- `test` runs the tests with cargo-nextest. On pull requests into `main` it runs a fast tier that leaves out the long scenario tests; the merge queue, pull requests into and pushes to `release/**` (no merge queue there), the nightly run and manual runs execute every test.
- A nightly run on `main` executes the full set and keeps build caches readable by pull requests; the merge queue no longer saves caches that pull requests cannot read.
- A contract test checks the triggers, the tiers and the pinned nextest.

## Impact
- Specification: release-process (Required CI checks; nightly run).
- `.github/workflows/ci.yml`, `.config/nextest.toml`, `scripts/test_ci_workflow.py`, README.
