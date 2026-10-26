# Diagnose specification merge readiness

## Why
GitHub can retain an old test merge when a pull request conflicts with dev. Treating every stale merge as a computation delay produces a misleading re-run instruction, while immediately rejecting a pending merge causes avoidable failures after a push.

## What Changes
- Distinguish conflicts from pending and stale test merges using the pull request API.
- Wait up to approximately 60 seconds for the exact candidate merge, with deterministic fixture coverage.
- Prepare identical trusted merge-fetch logic for deployment from main; the workflow remains absent from dev.

## Impact
- Adds specification merge readiness diagnostics and a standalone Python helper with fixtures.
- Trusted workflow deployment in main must grant pull-requests read permission and invoke the content-pinned helper; deployment is a separate explicit step.
