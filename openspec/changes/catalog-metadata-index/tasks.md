# Tasks

## 0. Decision

- [ ] 0.1 Choose option A or B in design.md with the owner and record it (TILR-15)

## 1. Catalog writer

- [ ] 1.1 Write the latest-release summary into the catalog plugin index during catalog generation and registration, under the catalog lock; unit test asserts it matches the latest release manifest and that the bumped `schema` is required on read
- [ ] 1.2 Keep immutability and atomicity: a rejected generation writes no index change; verify with the existing tree-hash assertions

## 2. Reader and docs

- [ ] 2.1 `browse` uses the summary and requests no `release.json`; test counts HTTP requests against a loopback server; a broken summary of one package leaves only that package without metadata
- [ ] 2.2 Update README "Plugin metadata" (cost paragraph), the catalog format description and "Changed in" (BREAKING format)
- [ ] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
