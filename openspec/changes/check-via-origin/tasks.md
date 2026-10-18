# Tasks

## 1. Library

- [ ] 1.1 `check(root, Option<&str>) -> CheckReport` (rows, replacements, unsatisfied); catalog row when listed, else origin row via one `git ls-remote --tags` per source in `exec.rs`; verify `check::check_without_catalog_reads_each_origin` (no fetch: clone refs unchanged) and `check::read_state_never_creates_or_locks_a_workspace`
- [ ] 1.2 Origin statuses `newer_than_origin`, `no_releases`, `origin_unreachable`; remove `not_in_catalog`; `CheckRow.source`; verify a test with an unreachable source that still reports the other rows
- [ ] 1.3 `ForkOf.source` (optional on read, non-empty when present); `fork` writes the original's recorded source; upstream from catalog or `forkOf.source`; verify `model::manifest_rejections_have_precise_kinds` (empty source) and `check::fork_upstream_from_fork_of_source`

## 2. CLI

- [ ] 2.1 `check` without a catalog prints the table from the report (source per row, Replacement and Unsatisfied lines from the report); remove the missing-catalog refusal for `check`; verify `check_shows_fork_upstream_and_other_catalogs` and a new CLI test without `--catalog`

## 3. FFI

- [ ] 3.1 Free `check(root, catalog: Option<String>) -> CheckReport` with `source` and upstream `source`; verify `records_convert_field_by_field`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: catalog-less `check` case and fork upstream from `forkOf.source`; verify `./itests/dt --only scenarios`
- [ ] 4.2 README: remove the check workaround, document origin rows and statuses, "Changed in 0.6" (BREAKING `check` signature, `not_in_catalog` removed); verify `grep -n "not_in_catalog\|check needs a catalog" README.md` finds only "Changed in" lines
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
