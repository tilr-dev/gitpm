# Tasks

## 1. Library

- [x] 1.1 New `state.json` via temp + rename (TILR-52); verify `workspace::open_creates_the_workspace_layout_and_lock`

## 2. Specs, tests, docs

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`

## 3. Supervisor follow-up

- [x] 3.1 Share the atomic writer between catalog and workspace; clean abandoned initial-state temporaries under the workspace lock
- [x] 3.2 Verify deterministic crash cleanup and workspace isolation; run self-review and bin/finish

## 4. Supervisor follow-up 2

- [x] 4.1 Stage the initial state inside the root; clean only gitpm's own temporaries when they are the root's sole content
- [x] 4.2 Verify mount-point roots (EXDEV hook) and crash cleanup; run self-review and bin/finish
