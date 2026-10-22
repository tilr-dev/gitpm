# Tasks

## 1. Library

- [x] 1.1 Lock: check the process-wide set by canonical path before opening the lock file; never open and close a second descriptor of a held lock file (or switch to `flock`/OFD locks), for the workspace and the catalog lock (TILR-45); verify `lock::refused_second_open_keeps_the_first_lock` (a refused in-process open, then a child process still gets `locked`) and the same for two catalog writers

## 2. Specs, tests, docs

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
