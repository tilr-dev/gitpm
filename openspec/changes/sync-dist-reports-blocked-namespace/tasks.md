# Tasks

## 1. Library

- [x] 1.1 Blocked namespace paths put their active packages in `skipped` (TILR-51); verify `a_namespace_path_that_is_not_a_folder_blocks_nothing_else` extended with `sync-dist` failing `not_deployed`

## 2. Specs, tests, docs

- [x] 2.1 Dagger: blocked namespace with `sync-dist`; verify `./itests/dt --only scenarios`
- [x] 2.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`

## 3. Review corrections

- [x] 3.1 Choose the CLI hint from the skipped reason and verify blocked file and symlink recovery, preserving missing-clone advice
- [x] 3.2 Document blocked namespace warnings and sync-dist exit status in README
- [ ] 3.3 Run self-review, bin/check and Dagger blocked namespace scenario
