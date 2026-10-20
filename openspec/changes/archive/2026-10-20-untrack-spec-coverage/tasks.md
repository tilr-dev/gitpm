## 1. Specification

- [x] 1.1 Modify the `integration-tests` "Coverage report" requirement: the report is local, `openspec/coverage/` is ignored by Git and never committed; add the scenario "Report stays untracked". Verified by `openspec validate untrack-spec-coverage --strict`.

## 2. Repository and documentation

- [x] 2.1 Remove `openspec/coverage/coverage.md` and `coverage.json` from the index and add `/openspec/coverage/` to `.gitignore`. Verified by task 3.1.
- [x] 2.2 Say in `README.md`, `itests/README.md` and the `itests/tools/coverage.py` docstring that the report is written locally and not committed. Verified by task 3.2.

## 3. Verification

- [x] 3.1 Run `./itests/dt coverage` and verify it writes both files, `git status --porcelain -- openspec/coverage` is empty, `git check-ignore openspec/coverage/coverage.md openspec/coverage/coverage.json` lists both and `git ls-files openspec/coverage` is empty.
- [x] 3.2 Verify no tracked file outside `openspec/changes/archive/` asks to regenerate and commit the report: `rg -n "openspec/coverage"` lists only the spec, this change, the generator and the READMEs, each saying the report is local.
- [x] 3.3 Run `openspec validate --all --strict`, then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` through `bin/check`; archive through `bin/finish`.
