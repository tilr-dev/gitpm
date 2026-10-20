## MODIFIED Requirements

### Requirement: Coverage report
`./itests/dt coverage` SHALL write `openspec/coverage/coverage.md` and `openspec/coverage/coverage.json` without running Dagger, listing per capability each requirement and scenario with the Dagger cases tagged for it, the cargo tests the spec text names, and the last run's status when a run result exists; requirements with neither SHALL be listed as uncovered. The report is a local artifact generated on demand: the whole `openspec/coverage/` directory SHALL be ignored by Git and nothing under it SHALL be committed.

#### Scenario: Uncovered requirement
- **WHEN** a requirement has no tagged case and names no test
- **THEN** it appears under "Uncovered" in `coverage.md` and with `"covered": false` in `coverage.json`

#### Scenario: Report stays untracked
- **WHEN** `./itests/dt coverage` has written the report in a clean checkout
- **THEN** both files exist, `git status --porcelain -- openspec/coverage` prints nothing and `git ls-files openspec/coverage` lists no file
