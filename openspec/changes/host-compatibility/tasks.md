# Tasks

## 1. Library

- [ ] 1.1 Manifest `host` map (validation, write-back, reported shape); verify `model::manifest_host_ranges_are_validated` and `list_reports_metadata_present_or_absent_and_filters_by_provides` (`host: {}`)
- [ ] 1.2 `Options.host`; host-aware selection in install, `install-catalog`, `download`, update and `update all`; `Error::IncompatibleHost` (kind `incompatible_host`, README kinds); verify `workspace::releases_are_chosen_for_the_host`, `error::every_variant_has_its_stable_kind` and `error::readme_lists_exactly_the_error_kinds`
- [ ] 1.3 `check` notes `needs_host`; verify `catalog::check_names_the_host_a_newer_release_needs`

## 2. CLI

- [ ] 2.1 `--host NAME@VERSION` / `GITPM_HOST` passed as `Options.host`; verify `gitpm_variables_set_root_catalog_and_sources` extended with `GITPM_HOST`

## 3. FFI

- [ ] 3.1 `ManagerOptions.host`, `PmError.IncompatibleHost`, `host` in records, `needsHost`; verify `records_convert_field_by_field` and `every_library_error_maps_to_its_own_case_with_the_full_message`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: releases for two host versions, `update all` kept back by host; verify `./itests/dt --only scenarios`
- [ ] 4.2 README manifest (`host`), settings (`--host`) and "Changed in 0.6"; verify by reading against the specs
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
