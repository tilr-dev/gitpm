# Tasks

## 1. Library

- [ ] 1.1 fsync journal, `state.json` and parent folders at commit (TILR-54); verify `txn::commit_syncs_journal_state_and_parents` (sync calls through a test hook)

## 2. Specs, tests, docs

- [ ] 2.1 README rules: definition of a crash (process death or power loss); verify by reading against the specs
- [ ] 2.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
