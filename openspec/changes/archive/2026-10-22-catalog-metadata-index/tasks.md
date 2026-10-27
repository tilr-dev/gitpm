# Tasks

## 1. Catalog writer

- [x] 1.1 Write the latest-release summary (`version`, metadata) into the catalog package index during catalog generation and registration, under the catalog lock; verify `catalog::index_summary_matches_the_latest_release` (summary equals the latest release manifest)
- [x] 1.2 Keep immutability and atomicity: a failed generation (for example a broken newest tag) writes no index change; verify with the existing tree-hash assertions

## 2. Reader and docs

- [x] 2.1 `browse` uses the summary and requests no `release.json`; verify `catalog::browse_reads_only_indexes` (HTTP requests counted against a loopback server: one index per listed package, no `release.json`) and that a broken summary of one package leaves only that package without metadata
- [x] 2.2 Update README "Plugin metadata" (cost paragraph), the catalog format description and "Changed in 0.6" (index summary added without a schema bump; regenerate)
- [x] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
