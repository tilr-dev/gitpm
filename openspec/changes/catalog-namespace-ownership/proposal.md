# Proposal: Namespace ownership in the catalog

## Why

The catalog was meant to accept releases from anyone, with verified and unverified plugins told apart (UC1 in Basic Memory "Tilr — менеджмент плагинов: use cases"); who may publish into a namespace, and a verified flag, were noted as missing (Codex sessions 2026-10-14/15). Verified on `dev`: a plugin ID is bound to the first source that registered it ("registered from a different source"), but nothing ties a namespace to an owner, so the first source to register `tilr/x` owns that ID. Rationale beyond that was not recorded and no design exists.

## What Changes

To be designed; candidate scope:
- An optional namespace policy in a local catalog (namespace → allowed source patterns) enforced by `catalog-update` and `register`.
- Reporting the policy result in `info`/`browse` (for example a `verified` field) without changing install behavior.

## Capabilities

### Modified Capabilities
- `catalog` — deltas to be written after the design; `skip_specs: true` until then.

## Out of scope

- Hosted publishing APIs, accounts, signatures (catalogs stay static files written locally).
- Badges, telemetry (UC9/UC10, excluded earlier).

## Impact

`crates/gitpm/src/catalog.rs`, catalog format (additive), README catalog section.
