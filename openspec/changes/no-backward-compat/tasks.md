# Tasks

## 1. Library

- [ ] 1.1 IDs: accept only `namespace/name`; remove the unscoped branch and the "ID conflicts with an installed namespace path" check; verify `model::valid_id_accepts_scoped_ids_only` (`demo1` → `invalid_id`)
- [ ] 1.2 Release tags: `identity::release_tag` always returns `ID@VERSION`; release discovery ignores `vVERSION` in single- and multi-package repositories (no warning); verify `identity::release_tag_is_id_at_version` and a test where a repository with only `v1.0.0` fails install with `no_releases`
- [ ] 1.3 State format: refuse a file without `format` or with another format with `unsupported` without rewriting it; verify `model::state_format_other_than_current_is_refused` (format 1 and 3, bytes unchanged)
- [ ] 1.4 Catalog: remove the v1 reader, `catalog::migrate`, `legacy/<id>/` paths and the "release without files" exception; a v1 `catalog.json` fails with `catalog`; verify `catalog::v1_catalog_is_refused` and `catalog::release_without_hashes_is_refused`
- [ ] 1.5 Metadata: `list` reports `resolved[]` metadata from the record only; verify `list_reports_metadata_present_or_absent_and_filters_by_provides` with a record without `metadata` (absent shape, clone not read)

## 2. CLI

- [ ] 2.1 Remove `create` and `migrate-catalog` from the command table and help (BREAKING); verify `help_names_the_binary_and_removed_commands_are_unknown` (`unknown command: create; use --help`)

## 3. FFI and release

- [ ] 3.1 `crates/gitpm-ffi/Cargo.toml` takes the library version; add it to release-please extra-files; verify the release PR dry run (`release-please release-pr --dry-run`) lists the FFI manifest, and `cargo metadata` shows equal versions

## 4. Specs, tests, docs

- [ ] 4.1 Update unit/CLI tests that use legacy IDs, `v*` tags or format-1 state; verify `cargo test --workspace`
- [ ] 4.2 Dagger: rewrite cases that use unscoped IDs, `v*` tags, `create` or `migrate-catalog`; verify `./itests/dt --only scenarios` green
- [ ] 4.3 README "Changed in 0.6": BREAKING entries with migrations (IDs, tags, state format, v1 catalogs, `create`); "Rules of the library" lists the state format per release; verify `grep -n "legacy\|migrate-catalog\|vX.Y.Z" README.md` hits only "Changed in" lines
- [ ] 4.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
