## ADDED Requirements

### Requirement: Workspace layout changes require the full integration suite
Changes to managed repository storage, package worktrees or transaction activation
SHALL run the complete `./itests/dt` suite before review and merge. Contributor
instructions SHALL require this gate in addition to the default checks, without
changing the shared workflow's default check set.

#### Scenario: Layout change ready for review
- **WHEN** a change affects bare repositories, package worktrees or their activation journal
- **THEN** the contributor runs the full Dagger suite and verifies that its successful result belongs to the final committed revision before submitting it for review or merge
