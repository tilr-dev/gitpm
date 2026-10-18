# Proposal: check works without a catalog, through each package's origin

## Why

The owner decided on 2026-10-18 (decision D3, TILR-30) that a catalog is optional: a direct install records its source, and `check` must query that recorded origin instead of needing a catalog. Today `check::check(root, catalog_location)` requires a catalog and fails with `invalid_argument` ("no catalog configured") on a non-empty workspace (spec `catalog` "A catalog is optional"), packages missing from the catalog are only `not_in_catalog`, and README documents a workaround. A fork's upstream is known only from the catalog because `forkOf` has no source (review finding G2#7, folded into TILR-30).

## What Changes

- **BREAKING** One `check(root, catalog: Option)` returning a `CheckReport` (rows, replacement bindings, unsatisfied dependencies). Per package: the catalog row when a catalog is given and lists the package, otherwise an origin row from `git ls-remote --tags` of the recorded source (no fetch, nothing written).
- New origin statuses `newer_than_origin`, `no_releases`, `origin_unreachable` (per row; never fails the whole check); `not_in_catalog` goes away (such packages get an origin row). Every row carries `source` (`catalog`/`origin`).
- `fork` writes `forkOf.source` (the original's recorded source); `check` uses it for the fork upstream when the catalog does not list the original. `forkOf.source` is optional when reading.
- CLI: `check` no longer needs a catalog. FFI: the free `check` returns the extended `CheckReport`.
- README: the "check needs a catalog" workaround is removed.

Depends on: `no-backward-compat`, `thin-cli-boundary` (FFI free `check`, neutral messages).

## Capabilities

### Modified Capabilities
- `catalog`: "check compares installed with the catalog" (renamed), "A catalog is optional".
- `updates`: "Updates need no catalog".
- `package-manifest`: "Known manifest keys" (`forkOf.source`).
- `fork-replacement`: "Fork metadata", "check shows fork upstream".
- `git-safety`: new "Origin tag lookup is bounded and restricted".
- `cli`: "Commands that need a catalog say so".
- `ffi-bindings`: new "Check report record" (adds the `CheckReport` record to the records of `thin-cli-boundary`).

## Out of scope

- Branch and pinned modes in `check` (`single-ref-mode`).
- Verified catalogs and "not in catalog" for protected packages (`protected-catalog`).
- Verifying the manifest of the newest origin tag during `check` (done at update).
- Comparing the commit of an origin tag with the installed commit: origin tags are compared by name, so a moved tag of the installed version is `up_to_date` (warnings for moved and retracted tags come with `no-tag-immutability`).

## Impact

`check.rs` (report, origin rows), `exec.rs` (ls-remote helper, same bounds as other git calls), `workspace/fork.rs` (`forkOf.source`), `model.rs` (`ForkOf.source`), CLI `check` printing, FFI `CheckReport`, README check section and "Changed in 0.6".
