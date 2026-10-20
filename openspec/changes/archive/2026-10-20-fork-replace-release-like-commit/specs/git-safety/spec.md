## ADDED Requirements

### Requirement: Library-owned Git input
Exception to "Git never waits for input": a Git ref transaction (`update-ref --stdin`) SHALL get only library-generated input instead of closed stdin, and the input pipe SHALL be closed after writing; user stdin SHALL NOT be inherited, and the other bounds of that requirement (no prompts, lowSpeed limits, batch SSH, 600s kill) SHALL apply unchanged.

#### Scenario: Library-owned ref transaction input
- **WHEN** release tags are created by `update-ref --stdin`
- **THEN** only library-generated commands are supplied, followed by EOF, with the same execution timeout
- **AND** verified by `fork_replace_release_late_tag_lock_leaves_no_tags`
