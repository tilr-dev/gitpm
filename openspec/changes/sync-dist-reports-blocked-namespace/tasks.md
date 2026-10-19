# Tasks

## 1. Library

- [ ] 1.1 Blocked namespace paths put their active packages in `skipped` (TILR-51); verify `a_namespace_path_that_is_not_a_folder_blocks_nothing_else` extended with `sync-dist` failing `not_deployed`

## 2. Specs, tests, docs

- [ ] 2.1 Dagger: blocked namespace with `sync-dist`; verify `./itests/dt --only scenarios`
- [ ] 2.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
