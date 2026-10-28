## 1. Apple release artifact
- [x] 1.1 Add strict release builds and packaging with checksum; verify both targets,
  missing prerequisites and build failures with stubbed tool integration tests.
- [x] 1.2 Wire tagged stable and candidate builds into the upload gate; exercise the
  workflow commands with stubs and verify archive contents and checksum.
- [x] 1.3 Document artifact naming, contents and local/release build behavior in README.

## 2. Validation
- [x] 2.1 Run the release workflow tests.
- [x] 2.2 Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings
  and cargo test --workspace through the task's final validation gate.
