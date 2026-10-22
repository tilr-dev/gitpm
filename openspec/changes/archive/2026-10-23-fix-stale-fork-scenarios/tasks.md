## Scenario corrections

- [x] Align local-work assertions with recorded-source isolation; verify with the focused uc_023 case.
- [x] Seed source branch collisions and preserve installation-only branches; verify with focused uc_024 cases.
- [x] Assert current refusal diagnostics and unchanged existing destinations; verify with focused uc_025 cases.
- [x] Assert newest installed repository revision and installed package revisions; verify with focused uc_072 case.
- [x] Confirm assertion sensitivity to broken fork behavior.

## Validation

- [x] Run the focused Dagger scenarios.
- [x] Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, and the full ./itests/dt through the finish checks.
