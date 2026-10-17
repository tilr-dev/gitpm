# Proposal: Port the test-bench scenarios into gitpm's integration tests

## Why

The bench `~/projects/tilr/plugins-gitpm-test` dogfoods gitpm with ten Tilr packages in one multi-package repository (scripts `make-repo.sh`, `install.sh`, `fork-demo.sh`, `assemble-site.sh`) and found most of the 0.4/0.5 CLI friction (Basic Memory notes on the bench and the CLI backlog). It depends on Tilr sources, a browser and a local gitpm build, so it is not part of CI. The individual behaviors are covered by `crates/gitpm-cli/tests/{monorepo,dist_active,metadata,lifecycle}.rs`, but the end-to-end user journey the bench runs is not one test, so a regression in how the steps combine would not be caught.

## What Changes

- A new CLI integration test file (for example `crates/gitpm-cli/tests/bench_journey.rs`) on synthetic packages only (no Tilr code, no network, hermetic like the other CLI tests), replaying the bench flow as one journey.
- Shared fixture helpers if the existing ones in the CLI tests do not fit.

## Capabilities

### Modified Capabilities
- None: tests only, no behavior change (`skip_specs: true`).

## Out of scope

- Anything Tilr-specific: assembling a site, `plugins.json`, load order, browsers, slots.
- Changing gitpm behavior; a failing step found by the port becomes its own change.
- Running the bench itself in CI.

## Impact

`crates/gitpm-cli/tests/`, test runtime (keep the whole suite around the current ~10 s).
