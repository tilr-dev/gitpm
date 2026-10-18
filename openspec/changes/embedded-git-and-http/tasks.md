# Tasks

## 0. Assessment

- [ ] 0.1 Inventory every git/curl call in `exec.rs` and check gix/ureq coverage, licenses (deny.toml) and MSRV; record findings in a design.md and write the planned `git-safety` and `catalog` deltas listed in proposal.md (TILR-20)
- [ ] 0.2 Start only after `catalog-streaming-size-limit` (TILR-12) is archived; rebase on its `exec::http_get`

## 1. Implementation

- [ ] 1.1 Add the embedded backend behind a feature; run the whole test suite against both backends (`cargo test --workspace` and `--features embedded`)
- [ ] 1.2 Verify credentials (SSH agent, credential helpers) on macOS without Command Line Tools; document results in README
- [ ] 1.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
