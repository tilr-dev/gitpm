# Tasks

## 0. Assessment

- [ ] 0.1 Inventory every git call in `exec.rs` and check coverage by gix, git2 and a hybrid (SSH authentication, agents and keys included), licenses (deny.toml) and MSRV; record findings in a design.md and write the planned `git-safety` deltas listed in proposal.md (TILR-141)

## 1. Implementation

- [ ] 1.1 Add the embedded git backend behind a feature; run the whole test suite against both backends (`cargo test --workspace` and with the feature)
- [ ] 1.2 Verify credentials (SSH agent, credential helpers) on macOS without Command Line Tools; document results in README
- [ ] 1.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
