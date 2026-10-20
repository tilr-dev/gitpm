## ADDED Requirements

### Requirement: Coverage report is local
The coverage report SHALL be a local artifact generated on demand: the whole `openspec/coverage/` directory SHALL be ignored by Git and nothing under it SHALL be committed.

#### Scenario: Report stays untracked
- **WHEN** `./itests/dt coverage` has written the report in a clean checkout
- **THEN** both files exist, `git status --porcelain -- openspec/coverage` prints nothing and `git ls-files openspec/coverage` lists no file
