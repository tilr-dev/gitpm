# Tasks

## 1. Library and FFI

- [ ] 1.1 Extract the source-independent part of `fork_packages` (tree copy, sibling removal, `gitpm.json` rewrite, dependency rewrite) so it can start from a fresh clone at a resolved revision; verify existing fork tests still pass (`cargo test -p gitpm fork`) (TILR-14)
- [ ] 1.2 Add a free function forking packages of a source at the latest release or a ref into `DIR` without a workspace; unit test on a two-package repository asserts only the chosen package is left, `forkOf.commit` is the resolved commit and no workspace folder exists
- [ ] 1.3 FFI `forkSource` over that function; FFI conversion test

## 2. CLI and docs

- [ ] 2.1 Parse `fork ID... --from SOURCE --ns NS --into DIR [--ref REF]`; refuse `--ref` without `--from`; CLI test asserts no workspace folder is created and the output lines match the installed-package fork
- [ ] 2.2 Document the form in README (fork section) and `crates/gitpm-cli/README.md`; verify the README example runs as written
- [ ] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
