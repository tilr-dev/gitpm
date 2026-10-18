# Tasks

## 1. Library

- [x] 1.1 IDs: accept only `namespace/name`; remove the unscoped branch and the "ID conflicts with an installed namespace path" check; verify `model::valid_id_accepts_scoped_ids_only` (`demo1` → `invalid_id`)
- [x] 1.2 Release tags: `identity::release_tag` always returns `ID@VERSION`; release discovery ignores `vVERSION` in single- and multi-package repositories (no warning); verify `identity::release_tag_is_id_at_version` and a test where a repository with only `v1.0.0` fails install with `no_releases`
- [x] 1.3 State format 3 with the key `packages`: refuse a file without `format` or with another format with `unsupported` without rewriting it; verify `model::state_format_other_than_current_is_refused` (no format, format 2 with `plugins`, format 4; bytes unchanged) and `model::state_round_trips_byte_for_byte` (`"format": 3`, `packages`)
- [x] 1.4 Catalog: remove the v1 reader, `catalog::migrate`, `legacy/<id>/` paths and the "release without files" exception; a v1 `catalog.json` fails with `catalog`; verify `catalog::v1_catalog_is_refused` and `catalog::release_without_hashes_is_refused`
- [x] 1.5 Metadata: `list` reports `resolved[]` metadata from the record only; verify `list_reports_metadata_present_or_absent_and_filters_by_provides` with a record without `metadata` (absent shape, clone not read)
- [x] 1.6 Fork: stop writing `pm.forkBase`/`pm.forkVersion` into a new fork's Git config (nothing reads them, TILR-105); verify `fork_creates_an_independent_author_checkout` asserts that `git config --local --get-regexp '^pm\.'` finds nothing

## 2. Rename Plugin → Package (TILR-43)

- [x] 2.1 Library: types and fields `Plugin*` → `Package*` (`InstalledPlugin` → `InstalledPackage`, `PluginSummary` → `PackageSummary`, `PluginIndex` and the rest), `State.plugins` → `State.packages` (serialized `packages`), every message "plugin …" → "package …" (for example "package ID is not installed"); the manifest file name stays `plugin.json`; verify `cargo test --workspace` and `grep -rn '"[^"]*plugin [^"]*"' crates/gitpm/src` hits only `plugin.json` literals
- [x] 2.2 FFI: `PluginManager` → `PackageManager`, `InstalledPlugin` → `InstalledPackage`, `PluginSummary` → `PackageSummary` and the other records; regenerate Swift/Kotlin bindings; verify `records_convert_field_by_field`, `manager_round_trip_on_empty_workspace` and `scripts/build-apple.sh` (generated `Gitpm.swift` has `PackageManager`, no `PluginManager`)
- [x] 2.5 Workspace folder of own clones `plugins/` → `packages/` (no compatibility: a workspace with `plugins/` is a format-2 workspace and refused); verify `workspace::open_creates_the_workspace_layout_and_lock` (`packages/` created, no `plugins/`)
- [x] 2.3 CLI: output and help say "package" (for example `dist/ already holds exactly the active packages.`); verify the CLI tests with the new texts
- [x] 2.4 Specs: before archive, add MODIFIED deltas for every remaining requirement of `openspec/specs/` that names `plugins.<id>`, `PluginManager`, `InstalledPlugin`, `PluginSummary` or a "plugin …" message (`grep -rn` over `openspec/specs/`), so archiving leaves no old literal name; verify `openspec validate no-backward-compat --strict`
  - Note: fix the `catalog` spec Purpose at archive (it still says "and readable v1 monolith"; a delta cannot change a Purpose).

## 3. CLI

- [x] 3.1 Remove `create` and `migrate-catalog` from the command table and help (BREAKING); verify `help_names_the_binary_and_removed_commands_are_unknown` (`unknown command: create; use --help`)

## 4. FFI and release

- [x] 4.1 `crates/gitpm-ffi/Cargo.toml` takes the library version; add it to release-please extra-files; verify the release PR dry run (`release-please release-pr --dry-run`) lists the FFI manifest, and `cargo metadata` shows equal versions
  - Note: `cargo metadata` confirms gitpm, gitpm-cli and gitpm-ffi at one version (0.5.0); `release-please release-pr --dry-run` reads the config from `main`, so it is verified after the merge to main.

## 5. Specs, tests, docs, config

- [x] 5.1 Update unit/CLI tests that use legacy IDs, `v*` tags or format-2 state; verify `cargo test --workspace`
- [x] 5.2 Dagger: rewrite cases that use unscoped IDs, `v*` tags, `create`, `migrate-catalog`, `plugins` in `state.json` or the `pm.forkBase`/`pm.forkVersion` assertions (`uc_023`, `uc_044`); remove `uc_065:legacy_release_may_lack_hashes`; verify `./itests/dt --only scenarios` green
- [x] 5.3 README "Changed in 0.6": BREAKING entries with migrations (IDs, tags, state format 3 and `packages`, the Plugin → Package API/FFI rename table, v1 catalogs, `create`); "Rules of the library" lists the state format per release; verify `grep -n "legacy\|migrate-catalog\|vX.Y.Z\|PluginManager\|InstalledPlugin" README.md` hits only "Changed in" lines
- [x] 5.4 `openspec/config.yaml`: add the rule "No backward compatibility before 1.0 (TILR-38): no aliases, legacy formats, migrations or compatibility shims; breaking changes are marked BREAKING" to `rules.proposal` and `rules.specs`; add the term rule "In spec prose say package; plugin appears only inside literal file or field names (`plugin.json`)" to `rules.specs`; in `context` drop "Until TILR-38 lands the code still reads them" and the removed legacy paths from "Compatibility", and update the glossary: literal names `packages`, `PackageManager`, `InstalledPackage`, `PackageSummary` and the workspace folder `packages/` (`plugin.json` stays), the term "catalog plugin index" becomes "catalog package index" (the file `namespaces/<ns>/<name>/plugin.json`), `packages.<id>.source`; verify `openspec validate --all --strict` and reading the file against the code
- [x] 5.5 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
