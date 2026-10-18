# Tasks

## 1. Library

- [x] 1.1 Remove moved-tag refusals in own and shared clone fetches and in `update all` (`clones::fetch_tags` loses its pinned-tag check, `clones::fetch_origin` is the one fetch of own clones); verify `workspace::moved_and_deleted_tags_are_taken_as_they_are` and the inverted `a_moved_tag_of_an_installed_package_does_not_block_a_sibling` (was `…_refuses_install_and_update_of_a_sibling`, now succeeds)
- [x] 1.2 Version-only comparison in release mode with the two warnings (moved, retracted) in `RevisionChange.warnings` and `Unchanged.warnings` of `UpdateAllPlan.unchanged` (`release_tag_warning`; every release tag deleted counts as retracted); verify `stable_update_never_downgrades`, `stable_update_follows_deleted_and_moved_upstream_tags`, `update_takes_a_moved_release_tag_at_its_new_commit`, `return_to_releases_at_the_installed_version_keeps_the_commit` and `update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases`
- [x] 1.3 `catalog-update` keeps published records and warns on moved tags and on manifest/file differences (never `version_conflict`), `UpdateSourcesReport.warnings`; verify the renamed `catalog_update_keeps_published_records_when_tags_move` and `catalog::update_sources_keeps_build_metadata_and_published_records`
- [x] 1.4 Rollback to a missing previous commit fails with `no_previous_version` naming ID and commit (`Error::NoPreviousVersion.commit`); verify `rollback_to_a_pruned_commit_names_it`
- [x] 1.5 Rollback classifies tags like update (published = on the origin with the same object at the last fetch, recorded under `refs/origin-tags/` after every clone and fetch by `lost_work::record_origin_tags`), offline (TILR-25); verify `rollback_of_a_fork_treats_fetched_upstream_tags_as_published` (fork → update → rollback succeeds; a hand-made tag still blocks both) and `update_and_rollback_refuse_hidden_local_work`

## 2. CLI

- [x] 2.1 Warnings printed from the results (no CLI logic): `Unchanged.warnings` in `update all`, `UpdateSourcesReport.warnings` in `catalog-update`; verify `update_after_a_deleted_release_tag_is_up_to_date_with_a_warning` (`up to date` + warning, exit 0, also `update all`) and `release_manifest_must_match_tag_and_update_warns_on_retagging`

## 3. FFI

- [x] 3.1 Warnings reach `RevisionChange.warnings` and `UpdateAllPlan.unchanged[].warnings`; `PmError.NoPreviousVersion.commit`; verify `records_convert_field_by_field`, `ids_and_lost_work_items_are_preserved` and a Swift build of `scripts/build-apple.sh` output

## 4. Specs, tests, docs

- [x] 4.1 Dagger: moved-tag and deleted-tag cases flip from failure to warning (`uc_015`, `uc_016`, `uc_032`), a fork → update → rollback case succeeds (`uc_017:rollback_of_a_fork_with_upstream_tags`), a missing previous commit names `no_previous_version`; anchors stay on current main slugs until archive; verify `./itests/dt --only scenarios`
- [x] 4.2 README "Changed in 0.6" (BREAKING: no tag policing) and the updates section; verify `grep -n "changed commit\|retagg" README.md` hits only "Changed in" lines
- [x] 4.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
