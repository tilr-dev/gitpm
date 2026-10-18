# Tasks

## 1. Library

- [ ] 1.1 `verified` flag read and preserved; `catalog-update` refuses a verified catalog; verify `catalog::verified_catalog_takes_only_registered_versions`
- [ ] 1.2 Install records `catalog` only for verified catalogs (also pulled dependencies); one-catalog rule among verified ones; verify `one_catalog_per_workspace` (verified/unverified cases) and `catalog_install_resolves_dependencies_and_records_edges`
- [ ] 1.3 Protected update path (catalog versions only, recorded location, commit + hash check) in `update` and `update all`; `--detach`; verify `workspace::protected_packages_update_only_to_catalog_versions` with a tampered release
- [ ] 1.4 `check`: protected rows from their catalog, `upstream_not_in_catalog`, `catalog_verified`; verify a check test with a newer origin tag

## 2. CLI

- [ ] 2.1 `update --detach`; check header `Catalog: LOCATION (verified|unverified: discovery only)`; verify CLI tests for both

## 3. FFI

- [ ] 3.1 `update(id, target, detach)`, `CheckReport.catalogVerified`, new status; verify `records_convert_field_by_field`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: verified catalog install → update to catalog-only versions → detach; tampered release refused on update; verify `./itests/dt --only scenarios`
- [ ] 4.2 README catalog section (verified vs discovery, register-only, detach) and "Changed in 0.6" (BREAKING: unverified catalogs no longer bind packages; catalog-update refused for verified); verify by reading against the specs
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
