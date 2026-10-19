# Tasks

## 1. Library

- [x] 1.1 Deploy and hash the committed tree without applying Git attributes; hash the staged export `dist/<id>` (TILR-46); verify a test with `export-ignore` and `export-subst` in `.gitattributes` (deployed bytes equal hashed bytes)

## 2. Specs, tests, docs

- [x] 2.1 Dagger: export-ignore deploy; verify `./itests/dt --only scenarios`
- [x] 2.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
