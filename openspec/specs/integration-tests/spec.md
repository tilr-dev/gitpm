# integration-tests Specification

## Purpose
Defines gitpm's black-box integration suite: a Dagger module in `itests/` that builds gitpm from this repository and runs its Rust tests, clippy and scenario cases against the `gitpm` binary, and the report that maps cases to spec requirements.

## Requirements

### Requirement: One entry point runs the suite
`./itests/dt` SHALL build gitpm from the working tree of this repository and run the Rust tests, clippy and every scenario case, each in its own container, and SHALL exit 0 only when no test failed. It SHALL accept `--only` (comma list of `rust`, `lint`, `scenarios`), `--filter TEXT` (substring of a test id), `--jobs N` and `--cached`, and SHALL take no arguments naming other checkouts.

#### Scenario: Filtered run
- **WHEN** the developer runs `./itests/dt --only scenarios --filter uc_015`
- **THEN** only the cases of `uc_015` (and the fixtures they need) run, and `itests/results/latest/summary.txt` lists them

#### Scenario: Failing case
- **WHEN** one scenario case fails
- **THEN** `./itests/dt` prints the summary with the failing id and its log tail and exits non-zero

### Requirement: Engine selection
`./itests/dt` SHALL run on the pb engine through `dagger-pb` by default and on a local engine when `DT_ENGINE=dagger` is set, and SHALL refuse to start when the Dagger CLI is not the pinned version (v0.21.10) or when a `.env` file exists in `itests/` or any parent directory, naming the problem.

#### Scenario: Wrong CLI version
- **WHEN** the installed Dagger CLI is v0.22.0
- **THEN** `./itests/dt` exits non-zero with a message naming v0.21.10, before contacting an engine

#### Scenario: Stray env file
- **WHEN** a `.env` file exists in the repository root
- **THEN** `./itests/dt` refuses and names that file

### Requirement: Scenarios drive only the gitpm binary
Scenario cases and fixtures SHALL invoke only the `gitpm` binary built from this repository (and `gitpm-api` from `crates/gitpm/examples/api-parity.rs`), configured through `GITPM_ROOT`, `GITPM_CATALOG` and `GITPM_SOURCES`; they SHALL NOT use the stand CLI, `PM_*` variables, network access beyond loopback, or real hosting services. The registry SHALL refuse a case or fixture that references `PM_*` variables or the `pm` binary.

#### Scenario: Legacy reference
- **WHEN** a scenario file calls `t.run("pm", "list")` or sets `PM_ROOT`
- **THEN** `python3 -m gitpmtest list` fails naming the case, and the run stops before any test

### Requirement: Every case names the spec it verifies
Every scenario case SHALL carry at least one tag `CAPABILITY/REQUIREMENT-SLUG[#SCENARIO-SLUG]` that resolves to a requirement (and scenario) in `openspec/specs/`, or the explicit tag `none:REASON`. An untagged case or a tag that resolves to nothing SHALL fail the registry.

#### Scenario: Unknown tag
- **WHEN** a case is tagged `dist/no-such-requirement`
- **THEN** the registry fails naming the case and the tag

### Requirement: Coverage report
`./itests/dt coverage` SHALL write `openspec/coverage/coverage.md` and `openspec/coverage/coverage.json` without running Dagger, listing per capability each requirement and scenario with the Dagger cases tagged for it, the cargo tests the spec text names, and the last run's status when a run result exists; requirements with neither SHALL be listed as uncovered.

#### Scenario: Uncovered requirement
- **WHEN** a requirement has no tagged case and names no test
- **THEN** it appears under "Uncovered" in `coverage.md` and with `"covered": false` in `coverage.json`

### Requirement: Coverage report is local
The coverage report SHALL be a local artifact generated on demand: the whole `openspec/coverage/` directory SHALL be ignored by Git and nothing under it SHALL be committed.

#### Scenario: Report stays untracked
- **WHEN** `./itests/dt coverage` has written the report in a clean checkout
- **THEN** both files exist, `git status --porcelain -- openspec/coverage` prints nothing and `git ls-files openspec/coverage` lists no file

### Requirement: Known gaps are visible, not hidden
A case MAY be marked as an expected failure naming the change that fixes it; such a case SHALL be reported `XFAIL` when it fails and SHALL count as a failure (`XPASS`) when it passes. A case that needs a capability absent from the runtime SHALL be reported `BLOCKED`, not passed.

#### Scenario: Streaming size limit
- **WHEN** `uc_065:oversized_streamed_catalog_response` is marked expected-failure for `catalog-streaming-size-limit` and still fails
- **THEN** the summary shows it as `XFAIL` and the run exits 0 if nothing else failed

#### Scenario: Fixed gap
- **WHEN** that change lands and the case passes
- **THEN** the summary shows `XPASS` and the run exits non-zero until the marker is removed
