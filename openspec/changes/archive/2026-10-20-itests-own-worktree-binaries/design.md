## Established cause (2026-10-20)

The source snapshot is mounted at the same path in every container. Cargo uses
source mtimes to detect modifications, while target outputs live in the shared
mutable `gitpm-itests-target-test` volume. An older source timestamp can therefore
make Cargo accept another snapshot's executable. Cargo's lock covers each Cargo
command; it does not cover the subsequent Python copies into `/out`, so sharing
outputs between different sources also permits replacement after compilation.

The controlled regression uses two dependency-free Cargo workspaces with identical
package metadata, equal-sized but distinguishable CLI/API/test sources, and source
mtimes explicitly set to epoch second 1. A is built first; then B builds against the
same target volume. Before the fix Cargo reports only `Finished` for B and its CLI
prints `a` instead of `b`. This is a sequential failure, independent of scheduling.

Recheck: `env -u SSH_AUTH_SOCK ./itests/dt call cache-regression` on the pb engine.
On 2026-10-20 the regression failed without the source digest suffix with
`replace: gitpm: expected b, got a`, and passed with the suffix. Evidence in task
workspace: `logs/cache-regression-before.log`, `logs/cache-regression-after.log`.
The test directly controls the stale-mtime precondition; it does not use sleeps.

## Cache ownership

Use the digest of the exact mounted `_build_src()` directory in the target cache
key, separately for build/test and clippy. This covers Cargo manifests, lockfile,
configuration, tests and sources. Specification-only edits are excluded. Different
sources cannot mutate one another's outputs; identical sources can safely share
Cargo's target lock and binaries. `/out` remains an immutable Dagger layer.

Alternatives: locking the entire build/copy sequence alone does not fix Cargo
freshness; forcing clean builds discards compilation reuse. Snapshot namespacing
keeps compiled outputs reusable for identical snapshots and shares Cargo downloads,
at the cost of separate target caches (including dependencies) when sources change.
No claim is made to incremental target reuse across different source snapshots.

## Regression coverage

The test uses the production `_rust` setup and `BUILD` script. Each invocation uses
a fresh cache prefix, seeds A, then checks B, runs A/B concurrently, and forces a
new build exec for unchanged B. It asserts CLI and API output, Rust test identity
and successful execution, and absence of `Compiling` on the warm exec. Specification
text changes each phase, proving it does not invalidate compilation reuse.
