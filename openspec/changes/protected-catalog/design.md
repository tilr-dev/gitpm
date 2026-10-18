# Design

## Context

Today every `install-catalog` verifies the release against the catalog and records `plugins.<id>.catalog`; afterwards updates read only the origin (`updates` "Updates need no catalog"), so the hash check protects the first install only. Decision D4 (TILR-34) ties protection to a verified catalog and keeps unverified catalogs as discovery.

## Open points, resolved

1. **Where the flag lives and who sets it.** `"verified": true` in the top-level `catalog.json` only; the owner edits it by hand. Writers (`register`, and `catalog-update` for unverified catalogs) read and preserve it. No CLI command sets it (a later change may add one).
2. **`catalog-update` into a verified catalog: refused.** Bulk generation would publish every tag of every source without review, which defeats "entries via register". `register` stays the only writer. Alternative: allow it with a confirmation flag — rejected as a second path to the same risk.
3. **Unverified `install-catalog` still verifies at install.** The record names a commit and hashes; refusing a mismatch costs nothing and keeps `install-catalog` meaning "exactly this release". It records no `catalog`, so updates follow the origin like a direct install. Alternative: skip verification for unverified catalogs — rejected (silent weakening).
4. **One-catalog rule only among verified catalogs.** Protection needs one source of truth per workspace; discovery does not. Installing from a second verified catalog is refused with `catalog`; unverified ones are free.
5. **Namespace ownership.** Folded in: a verified catalog is written only by its owner via `register`, and the existing rule "a package ID stays bound to the source that first registered it" prevents another source from taking an ID. No separate policy file.
6. **Detach.** `update ID --detach`: one transaction that removes `catalog` from the record and updates from the origin. A separate `detach` command was considered; the flag keeps the command set small and the action explicit.
7. **Which catalog an update reads.** The recorded canonical location, not `Options.catalog`: a protected package must not change its source of truth because the user pointed the CLI elsewhere. An unreachable catalog fails the update with `http`.
8. **Update verification.** The candidate version's release record gives the commit; gitpm fetches the origin, checks that the commit exists and that manifest and package dist file hashes equal the record, then stages. `update all` uses the catalog's versions as candidates for protected packages and the origin's tags for the rest.
9. **check.** Protected packages are compared with their catalog; the origin is also listed with `git ls-remote` to report `upstream_not_in_catalog` when it has a newer release than the catalog. The report carries `catalog_verified` for the configured catalog.

## API

`catalog::Catalog { verified: bool, … }`; `Manager::update_with(id, target, UpdateOptions { detach: bool })`; `CheckReport.catalog_verified`; `CheckStatus::UpstreamNotInCatalog`. FFI `update(id, target, detach)`, `CheckReport.catalogVerified`.

## Transactions and crash recovery

Protected updates use the existing update transaction; verification happens before staging, so a mismatch leaves the workspace unchanged. Detach changes the record in the same `state.json` write as the revision. Catalog writers keep their lock and temp-file protocol; the flag is written with the rest of `catalog.json`.
