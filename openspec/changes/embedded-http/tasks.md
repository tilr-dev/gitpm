# Tasks

## 0. Assessment

- [ ] 0.1 Inventory every curl call in `exec.rs` and check ureq coverage (HTTPS-only, no redirects, timeouts, streaming size limit), licenses (deny.toml) and MSRV; record findings in a design.md and write the planned `git-safety` and `catalog` deltas listed in proposal.md (TILR-145)
- [ ] 0.2 Start only after `catalog-streaming-size-limit` (TILR-12) is archived; rebase on its `exec::http_get`

## 1. Implementation

- [ ] 1.1 Add the embedded HTTP backend behind a feature; run the whole test suite against both backends (`cargo test --workspace` and with the feature)
- [ ] 1.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
