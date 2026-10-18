# Tasks

## 1. Library

- [ ] 1.1 `Mode` in the record, record shape within `state.json` format 3 (no format bump: 0.6.0 ships one format); verify `model::state_round_trips_byte_for_byte` with all three modes
- [ ] 1.2 Install targets: release, `--ref` (tag/commit → pinned, origin branch → branch), dev (default branch name recorded); fresh-clone ref resolution against `origin/*` (TILR-47); verify `workspace::install_records_the_ref_mode` including a branch-only ref
- [ ] 1.3 `update` follows the mode; `--ref`/`--dev`/`--release` switch it; pinned returns unchanged with reason `pinned`; verify `workspace::update_follows_and_switches_the_recorded_mode` and `branch_mode_follows_origin_head_and_refuses_diverged_history`
- [ ] 1.4 `update all` per mode (pinned in `unchanged` with reason, branch HEAD as single candidate); verify `update_all_follows_each_mode`
- [ ] 1.5 Forget writes a `forgotten` record for own clones; unified restore (record kept, commit or HEAD, lost work refused); verify `workspace::restore_keeps_the_forgotten_record` and a test with an unpublished commit
- [ ] 1.6 `check`: `pinned` rows, branch rows compared with the branch HEAD via ls-remote; verify a check test with three modes

## 2. CLI

- [ ] 2.1 Remove `--channel`; add `--dev` and `update --release`; mutual exclusion as usage errors; `ID pinned at V (REF)` lines; verify `usage_errors_name_the_problem_and_create_nothing` and `mutating_commands_print_one_line_per_package_and_no_ok_lines`

## 3. FFI

- [ ] 3.1 Replace `Channel` with `Target`; `InstalledPackage.mode`; unchanged reasons; verify `records_convert_field_by_field` and the Swift generation in `scripts/build-apple.sh`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: replace `--channel` cases with `--ref BRANCH`/`--dev`/`--release`; restore cases for own clones keeping `catalog`/`enabled`; verify `./itests/dt --only scenarios`
- [ ] 4.2 README "Changed in 0.6" (BREAKING `--channel`, FFI `Channel`, `mode` in the record, restore) and the install/update sections; verify `grep -n "\-\-channel" README.md` hits only "Changed in" lines
- [ ] 4.3 `openspec/config.yaml` glossary: replace "dev channel: `--channel dev` (FFI `Channel::Dev`), following the default branch HEAD" with the ref modes ("ref modes: `--ref TAG|COMMIT` pins, `--ref BRANCH` follows a branch, `--dev` follows the origin's default branch; FFI `Target`; the mode is recorded in state"); verify `grep -n "channel" openspec/config.yaml` finds nothing
- [ ] 4.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
