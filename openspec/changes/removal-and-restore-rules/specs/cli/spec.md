## MODIFIED Requirements

### Requirement: Errors and exit codes
On failure the CLI SHALL print `Error: MESSAGE` to stderr, for a library error also `Error kind: KIND` (and one `  - item` line per lost item of `local_changes` or `has_dependents`) and, for kinds where a gitpm command helps, one `Hint: …` line naming that command; CLI usage errors print no kind. It SHALL exit with status 1 on failure and 0 on success. Verified by `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: Unknown ID
- **WHEN** the user runs `gitpm update acme/none`
- **THEN** stderr has `Error: package acme/none is not installed` and `Error kind: not_installed`, and the exit status is 1

#### Scenario: Command hint
- **WHEN** the user installs `acme/greeter` again after forgetting it while its clone is retained
- **THEN** stderr has the neutral library message, `Error kind: already_installed` and a `Hint:` line naming `gitpm restore acme/greeter` and `gitpm uninstall acme/greeter`

#### Scenario: One refusal tells everything
- **WHEN** `gitpm uninstall lib` is refused because `app` depends on `lib` and the clone also has a modified file
- **THEN** stderr names `app`, lists the modified file as `  - …` and prints `Error kind: has_dependents`, from the one library error
