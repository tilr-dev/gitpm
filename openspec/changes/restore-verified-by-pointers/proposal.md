## Why

Six requirements lost their test pointers when their prose was rewritten. Restoring the pointers makes their existing automated coverage visible to readers and coverage tooling.

## What Changes

- Restore Verified by lines with existing test names in dist, package-manifest, multi-package-repos and catalog.
- Split requirements that exceed the strict 500-character body limit, moving existing prose and scenarios together without changing behavior.

## Impact

Specification text and scenario grouping only; no runtime or test behavior changes.
