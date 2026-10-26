## ADDED Requirements

### Requirement: Specification merge readiness diagnostics
Before fetching a pull request test merge for specification authorization, the trusted check SHALL read the pull request through the GitHub API with pull-requests read permission. It SHALL distinguish a conflict with dev from a pending or stale merge result, and SHALL inspect candidate Git objects without executing candidate code.

#### Scenario: Pull request conflicts with dev
- **WHEN** the API reports mergeable false for a head SHA matching the requested candidate SHA
- **THEN** the check fails immediately with a conflict diagnostic naming the task branch and instructing the author to merge dev with `git merge dev` and push
- **AND** it does not advise re-running the job

#### Scenario: GitHub is still computing the merge
- **WHEN** mergeable is null or the fetched merge commit does not have the requested candidate SHA as its second parent
- **THEN** the check waits and reads the API again for up to approximately 60 seconds
- **AND** it proceeds automatically once the API head SHA matches the requested candidate SHA, mergeable is true and the merge commit has the requested candidate SHA as its second parent
- **AND** it uses the first parent of that verified merge as the comparison baseline

#### Scenario: API response describes a stale head
- **WHEN** the API head SHA differs from the requested candidate SHA
- **THEN** the check treats that response as stale and reads the API again within the existing deadline
- **AND** it does not interpret its mergeable value as the candidate's conflict or readiness

#### Scenario: Command fails
- **WHEN** the API or Git command fails or times out
- **THEN** the check fails with a diagnostic identifying the command and preserving its error output

#### Scenario: Merge result remains stale
- **WHEN** the merge result remains pending or stale until the wait deadline
- **THEN** the check fails with the diagnostic that GitHub has not updated the merge result and advises re-running the job

#### Scenario: Deterministic regression coverage
- **WHEN** merge readiness decisions are tested
- **THEN** API responses, fetched parents, elapsed time and waits are controlled by fixtures without real waiting
- **AND** coverage includes a pending result becoming current, an immediate conflict, a stale API head becoming current, and stale results reaching the deadline
