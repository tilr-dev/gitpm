# Tasks

## 1. Library

- [x] 1.1 Refuse submodules (gitlinks) inside a package dist with `unsupported` before deploying (TILR-26); verify `a_submodule_in_dist_is_refused` (install fails, workspace unchanged)

## 2. Specs, tests, docs

- [x] 2.1 Dagger: submodule in dist refused; verify `./itests/dt --only scenarios`
- [x] 2.2 README "Changed in 0.6" (submodule in dist refused, BREAKING); verify by reading against the specs
- [x] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
