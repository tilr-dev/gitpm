# Tasks

## 1. Library

- [ ] 1.1 Identity via `git -c user.useConfigOnly=true var`; `invalid_argument` (TILR-49); verify `scaffold::derived_identity_is_refused` and `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`

## 2. Specs, tests, docs

- [ ] 2.1 README rules (identity) and "Changed in 0.6" (identity refusal, BREAKING); verify by reading against the specs
- [ ] 2.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
