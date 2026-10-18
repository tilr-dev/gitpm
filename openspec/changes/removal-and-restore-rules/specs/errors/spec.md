## MODIFIED Requirements

### Requirement: Display is the complete message
`Display` of an error SHALL be the complete human message in English. Library messages SHALL be neutral: no CLI command, flag or hosting service; the CLI adds hints per error kind (cli). Structured variants SHALL carry their data: `LocalChanges.items`, `HasDependents.dependents` and `lost_work`, `IncompatibleReplacement.consumers`, `NotDeployed.skipped`. Verified by `error::display_is_the_complete_message` and `error::messages_name_no_cli_command`.

#### Scenario: Has dependents
- **WHEN** disabling `lib` is refused because `app` depends on it
- **THEN** the error is `has_dependents`, its message names `app` and `dependents` is `["app"]`

#### Scenario: Neutral advice
- **WHEN** a scaffold refusal or a replacement refusal used to advise `init --multi` or `replace ORIGINAL --back`
- **THEN** the library message states the problem only (for example "the repository root is a package; a multi-package repository needs gitpm.json") and the CLI prints the command as a `Hint:` line

