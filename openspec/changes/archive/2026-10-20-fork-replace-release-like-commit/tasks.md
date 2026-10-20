# Tasks

## 1. Library

- [x] 1.1 `fork --replace` release step like `commit` (TILR-57); verify a hook-rejected release restores manifests/index with no tags, and a wrong-typed metadata key fails with `invalid_manifest`

## 2. Specs, tests, docs

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`

- [x] 2.2 Replace local fetch with update-ref stdin transaction; verify release with restricted file transport and all-or-none tag locking.
