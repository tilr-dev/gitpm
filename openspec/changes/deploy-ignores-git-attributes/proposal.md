# Proposal: Deploys and hashes ignore Git attributes

## Why

`git archive` honours `export-ignore` and `export-subst`, while catalog hashes and install verification are computed over the checked-out working tree, so deployed files can differ from hashed ones — a hole for the protected catalog (review 2, G1#15, TILR-46).

## What Changes

- Deploys copy the committed tree without applying Git attributes (`export-ignore`, `export-subst`).
- Catalog hashes and install verification (`install-catalog`, update of a protected package, `verify_catalog_release`) are computed over the staged export `dist/<id>`, the same bytes that are deployed.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

Shared requirement: dist "Deploys copy the committed tree" is also modified by `refuse-submodule-in-dist` (TILR-26); each delta changes only its own part of the current text. The change archived later rebases its delta onto the spec the earlier one left.

## Capabilities

### Modified Capabilities
- `dist`: "Deploys copy the committed tree".

## Impact

Deploy/export and hash code paths (`workspace.rs`, `catalog.rs`). Tracker: TILR-46. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).
