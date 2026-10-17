# Tasks

## 0. Assessment

- [ ] 0.1 Inventory every git/curl call in `exec.rs` and check gix/ureq coverage, licenses (deny.toml) and MSRV; record findings in a design.md

## 1. Implementation

- [ ] 1.1 Add the embedded backend behind a feature; run the whole test suite against both backends (`cargo test --workspace` and `--features embedded`)
- [ ] 1.2 Verify credentials (SSH agent, credential helpers) on macOS without Command Line Tools; document results in README
