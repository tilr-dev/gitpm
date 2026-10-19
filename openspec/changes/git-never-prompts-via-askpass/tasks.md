# Tasks

## 1. Library

- [x] 1.1 `exec.rs`: empty `GIT_ASKPASS` and `-c credential.interactive=never` for every git command (TILR-53); verify `exec::git_never_reads_stdin_or_prompts` (environment and args asserted)

## 2. Specs, tests, docs

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
