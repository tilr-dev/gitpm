## MODIFIED Requirements

### Requirement: update all never removes or downgrades
`update all` SHALL NOT remove or downgrade packages, SHALL keep replacement bindings and disabled flags, SHALL solve only healthy packages (a consumer whose dependencies are already unsatisfied is kept back) and SHALL report each kept-back package with its latest version and `HeldBack.reason`; a broken state SHALL NOT fail the run. Verified by `update_all_does_not_downgrade_or_accept_mutated_releases` and `update_all_skips_broken_consumers_and_never_reinstalls`.

#### Scenario: Kept back
- **WHEN** the newest release of a package does not fit a dependent's requirement
- **THEN** the CLI prints `ID kept back at V (latest L: dependency constraints)`, or, when it advances to an intermediate compatible release, `ID updated A → B (latest L kept back by dependency constraints)`, the reason taken from `HeldBack.reason`
- **AND** a package whose newer release needs a dependency that is not installed, with no catalog configured, is kept back with the reason `needs DEP REQ, which is not installed and no catalog is configured`

#### Scenario: Reasons
- **WHEN** a package is kept back
- **THEN** `HeldBack.reason` is one of `dependency constraints`, `needs DEP REQ, which is not installed and no catalog is configured`, `dependency DEP is not installed` (a broken consumer), `no longer replaces ORIGINAL` (a fork release without `replaces`) or `history of BRANCH diverged` (branch mode)

#### Scenario: Broken state does not fail the run
- **WHEN** a consumer's dependency was removed with force and the other packages have newer releases
- **THEN** `update all` updates the others, keeps the consumer back with its reason and exits 0; a single `update` of that consumer still fails with `dependency_conflict` (dependencies)
