# Tasks

## 1. Library

- [ ] 1.1 Remove moved-tag refusals in own and shared clone fetches and in `update all`; verify `releases::moved_and_deleted_tags_are_taken_as_they_are` and the inverted `a_moved_tag_of_an_installed_package_refuses_install_and_update_of_a_sibling` (now succeeds)
- [ ] 1.2 Version-only comparison in release mode with the two warnings (moved, retracted) in `RevisionChange.warnings` and `UpdateAllPlan.unchanged`; verify `stable_update_never_downgrades` and `stable_update_follows_deleted_and_moved_upstream_tags`
- [ ] 1.3 `catalog-update` keeps published records and warns on moved tags and on manifest/file differences (never `version_conflict`); verify the renamed `catalog_update_keeps_published_records_when_tags_move`
- [ ] 1.4 Rollback to a missing previous commit fails with `no_previous_version` naming ID and commit; verify `rollback_to_a_pruned_commit_names_it`
- [ ] 1.5 Rollback classifies tags like update (published = on the origin with the same object at the last fetch), offline (TILR-25); verify `rollback_of_a_fork_treats_fetched_upstream_tags_as_published` (fork → update → rollback succeeds) and `update_and_rollback_refuse_hidden_local_work` (a hand-made tag still blocks both)

## 2. CLI

- [ ] 2.1 Warnings printed from the results (no CLI logic); verify a CLI test for `update` after a deleted tag (`up to date` + warning, exit 0)

## 3. FFI

- [ ] 3.1 Warnings reach `RevisionChange.warnings` and `UpdateAllPlan.unchanged[].warnings`; verify `records_convert_field_by_field`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: moved-tag and deleted-tag cases flip from failure to warning; a fork → update → rollback case succeeds; verify `./itests/dt --only scenarios`
- [ ] 4.2 README "Changed in 0.6" (BREAKING: no tag policing) and the updates section; verify `grep -n "changed commit\|retagg" README.md` hits only "Changed in" lines
- [ ] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
