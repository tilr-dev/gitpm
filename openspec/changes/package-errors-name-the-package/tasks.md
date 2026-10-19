# Tasks

## 1. Library

- [ ] 1.1 Messages name the package: "COMMIT changed without a version bump", "history of BRANCH diverged", "rollback manifest mismatch", "package id changed upstream" and the `update all` local-work refusal carry the ID; FFI `PmError` gets `id` for them (TILR-50); verify `error::package_errors_name_the_package` and `ids_and_lost_work_items_are_preserved`

## 2. Specs, tests, docs

- [ ] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
