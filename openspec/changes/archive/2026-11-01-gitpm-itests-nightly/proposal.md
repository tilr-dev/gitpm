# Run the Dagger suite in CI

## Why
The black-box suite currently requires a manual run on pb. Hosted nightly and release-branch runs should catch regressions outside the required Rust checks.

## What Changes
- Add a nonrequired local-Dagger job on nightly main, manual runs and release-branch pushes.
- Bound compilation and test concurrency, restore engine caches, save them only on nightly main, and upload results even after failure.
- Document the hosted run and statically verify its workflow contract.

## Impact
- Affected spec: release-process (Nightly CI run).
- Affected code: CI workflow, integration-suite build concurrency, workflow contract tests and documentation.
