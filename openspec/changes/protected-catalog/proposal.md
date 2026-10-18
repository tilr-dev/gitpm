# Proposal: Protected (verified) catalogs

## Why

The owner decided on 2026-10-18 (decision D4, TILR-34; namespace ownership folded in) that file-hash integrity is a property of a verified catalog only: the owner marks a catalog `verified: true`, entries enter only through `register`, packages installed from it are hash-checked on install and on every update and update only to versions the catalog lists, an explicit flag detaches a package to "direct", and an unverified catalog is discovery only. Today `install-catalog` checks hashes once, then `update`/`update all` read the origin and move to unpublished releases without any check (review findings G3#1, G2#6), while `packages.<id>.catalog` only drives the one-catalog refusal and `check` "other catalog". With `no-tag-immutability` gitpm no longer polices tags, so this change is where integrity lives.

## What Changes

- `"verified": true` in the top-level `catalog.json`, set by the owner by hand and kept by writers; **BREAKING** `catalog-update` into a verified catalog is refused (`invalid_argument`), versions enter one by one via `register`. The existing ID-to-source binding is the namespace ownership. Into an unverified catalog `catalog-update` keeps working and never replaces a published record: a difference keeps the record with a warning (`no-tag-immutability`; owner decision of 2026-10-18 on TILR-34). One catalog per workspace stays.
- Protected packages: installed from a verified catalog (and dependencies pulled from it) record `catalog`; `update`/`update all` move them only to catalog-listed versions, checking commit, manifest and file hashes (`catalog` on mismatch), from the recorded location.
- `update ID --detach` (FFI `update(id, target, detach)`) drops the protection and updates from the origin. A protected package whose origin is ahead of its catalog stays with the new reason `not_in_catalog` (`update ID`, `update all`). **BREAKING** (FFI/API): `update` takes `detach`; `RevisionChange.detached`, `UnchangedReason::NotInCatalog`, `CatalogOverview.verified` are new fields/cases.
- **BREAKING** Unverified catalogs: discovery only. `install-catalog` still verifies at install but records no `catalog`; updates follow the origin; the one-catalog rule applies only among verified catalogs.
- `check`: protected packages compare with their catalog; a newer origin release that the catalog lacks shows `upstream_not_in_catalog` ("upstream V not in catalog"); the report says whether the catalog is verified (`catalog_verified`), `info` prints `Verified: yes|no`. `list` shows `catalog` only for protected packages.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`, `removal-and-restore-rules`, `no-tag-immutability`.

## Capabilities

### Modified Capabilities
- `catalog`: new "Verified catalogs"; "Bounded overview" (the `Verified:` line), "One catalog per workspace", "check compares installed packages with their catalog or origin".
- `install-uninstall`: "Install from the catalog".
- `updates`: new "Protected packages update through their catalog"; "Updates need no catalog".
- `dependencies`: "Install resolves dependencies".
- `workspace-layout`: "Installed package record".
- `cli`: "Command set", "JSON output for state and catalog".
- `ffi-bindings`: "Manager surface" (`update(id, target, detach)`); new "Catalog verification in records" (adds `CheckReport.catalogVerified` and the new status).

## Out of scope

- Signing catalogs or releases; a command to set the flag.
- Hosting or access control of catalogs (the owner's job).
- Per-namespace policy files.
- Hashing the `git archive` export tree instead of the checked-out `dist/` (TILR-46, change `robustness-fixes`); it applies to the install and the protected update path alike.

## Impact

`catalog.rs` (flag, writer refusal), `workspace.rs` (install records, protected update path, detach), `upgrade.rs` (catalog candidates), `check.rs`, CLI `--detach` and check header, FFI, README catalog section and "Changed in 0.6".
