# Tasks

## 1. Library

- [x] 1.1 `verified` flag read and preserved; `catalog-update` refuses a verified catalog; verify `catalog::verified_catalog_takes_only_registered_versions`
- [x] 1.2 Install records `catalog` only for verified catalogs (also pulled dependencies); one-catalog rule among verified ones; verify `one_catalog_per_workspace` (verified/unverified cases) and `catalog_install_resolves_dependencies_and_records_edges`
- [x] 1.3 Protected update path (catalog versions only, recorded location, commit + hash check) in `update` and `update all`; `--detach`; verify `workspace::protected_packages_update_only_to_catalog_versions` with a tampered release (also `workspace::detach_without_a_new_release_is_a_change`, `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`, `workspace::unverified_catalog_installs_follow_the_origin`, `workspace::new_dependencies_of_a_protected_package_come_from_its_catalog`, `workspace::install_from_a_verified_catalog_protects_a_direct_install_of_its_release`); `update all` keeps a package with an unusable catalog (`UnchangedReason::CatalogUnavailable`)
- [x] 1.4 `check`: protected rows from their catalog, `upstream_not_in_catalog`, `catalog_verified`; verify `workspace::check_compares_protected_packages_with_their_catalog` (newer origin tag) and `check_shows_fork_upstream_and_other_catalogs`

## 2. CLI

- [x] 2.1 `update --detach`; check header `Catalog: LOCATION (verified|unverified: discovery only)`; `info` line `Verified:`; verify `verified_catalog_protects_updates_and_detach_drops_it`

## 3. FFI

- [x] 3.1 `update(id, target, detach)`, `CheckReport.catalogVerified`, `CheckRow.originLatest`, new status, `CatalogOverview.verified`, `RevisionChange.detached`, `UnchangedReason.notInCatalog`; verify `records_convert_field_by_field`, `check_rows_carry_status_and_message`, `catalog_records_convert`; Swift bindings generated and built with `scripts/build-apple.sh`

## 4. Specs, tests, docs

- [x] 4.1 Dagger: verified catalog install → update to catalog-only versions → detach; tampered release refused on update; verify `./itests/dt --only scenarios` (`uc_057`: `update_keeps_original_source`, `protected_update_check_and_detach`, `tampered_release_refused_on_update`, `unverified_catalog_is_exempt`, `catalog_update_refused_for_a_verified_catalog`; fixtures `g4_two_catalogs` and `g6_journey` verified)
- [x] 4.2 README catalog section (verified vs discovery, register-only, detach) and "Changed in 0.6" (BREAKING: unverified catalogs no longer bind packages; catalog-update refused for verified); verify by reading against the specs
- [ ] 4.4 At archive: re-anchor the protected-catalog cases of `itests/scenarios/uc_057.py` at `catalog/verified-catalogs` and `updates/protected-packages-update-through-their-catalog`; verify `./itests/dt --only scenarios --filter uc_057` (registry check)
- [x] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
