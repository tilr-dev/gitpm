# Tasks

## 0. Specs

- [x] 0.1 Write the spec deltas for `package-ids`, `dependencies`, `updates`, `install-uninstall`, `authoring`, `catalog`, `cli` and `ffi-bindings` and drop `skip_specs`; verify `openspec validate pre-release-versions --strict` (TILR-41)

## 1. Library

- [ ] 1.1 npm requirement parser (bare = exact, `^`, `~`, space-separated AND, `*`); unit tests for each form and for 0.x carets (TILR-41)
- [ ] 1.2 SemVer versions with pre-releases in `version_key` and release discovery; ordering test `1.0.0-alpha < 1.0.0-beta < 1.0.0`; build metadata still refused with `invalid_version`
- [ ] 1.3 Selection: install/update/update all never pick a pre-release implicitly; explicit `--version`/`--ref` installs one; update then follows pre-releases of the same version and its final release; tests with and without opt-in
- [ ] 1.4 Major marking: `RevisionChange`/update-all plan carry a `major` flag; `check` marks `(major)`; tests
- [ ] 1.5 Fork rewrites keep writing `^V`; verify the fork dependency-rewrite tests still assert `^V` under the npm parser

## 2. CLI, FFI, docs

- [ ] 2.1 CLI prints `(major)` and accepts pre-release `--version`; CLI tests; FFI records expose the flag (parity)
- [ ] 2.2 README "Changed in" (BREAKING requirement syntax), versions section; Dagger cases tagged into the new requirements
- [ ] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
