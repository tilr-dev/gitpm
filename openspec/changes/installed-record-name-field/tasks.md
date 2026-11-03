## 1. Record contract and regression
- [x] 1.1 Include required `name` in the installed package record specification, consistent with the installed name snapshot contract.
- [x] 1.2 Update the exact-key assertion in `uc_017:only_one_previous_version_is_kept`; verify with `./itests/dt --only scenarios`.

## 2. Validation
- [x] 2.1 Review the diff with a subagent and resolve findings.
- [x] 2.2 Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings and cargo test --workspace through the finish gate.
