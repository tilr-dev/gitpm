## ADDED Requirements

### Requirement: A pending replacement of fork --replace is confirmed and resumed
When the replace step of `fork --replace` needs confirmation (incompatible consumers, not forced), the library SHALL stop that package after its install and return its `ReplacementPlan` as pending; `Manager::fork_replace_resume` (FFI `forkReplaceResume`) SHALL apply the pending replacements once confirmed, re-planning under the lock (`incompatible_replacement` when the consumers changed). Verified by `fork_replace_pending_replacement_is_resumed_after_confirmation`.

#### Scenario: Confirmed on a TTY
- **WHEN** `gitpm fork acme/hello --ns me --into f --replace` finds `app` needing `acme/hello ^2` and the user answers `y`
- **THEN** the CLI prints the warning from the pending plan, asks on stderr and calls the resume API, and `acme/hello` is replaced by `me/hello`

#### Scenario: No terminal
- **WHEN** the same command runs without a TTY and without `--force`
- **THEN** `me/hello` stays released and installed, `acme/hello` is not replaced, and the command fails with `incompatible_replacement`

### Requirement: Replacement plans carry what the output needs
`ReplacementPlan` (of `replace_plan`, `switch_back_plan`, `replace`, `switch_back` and pending fork replacements) SHALL carry `original`, `provider`, `version`, `current`, `consumers`, `incompatible`, `installed_version` (the installed version of `provider`) and `fork_enabled` (FORK is installed and enabled), so the replace and switch-back lines are built from the plan without reading state. Verified by `replacement_plan_carries_installed_version_and_fork_enabled`.

#### Scenario: Switch back to a disabled fork
- **WHEN** `me/greeter` is disabled and the user runs `replace acme/greeter --back`
- **THEN** the plan has `fork_enabled` false and the line ends `; me/greeter stays installed (disabled)`

#### Scenario: Fork version differs from the declared one
- **WHEN** `me/greeter` 0.3.0 declares `replaces: acme/greeter 1.4.0` and the user replaces `acme/greeter` with it
- **THEN** the plan has `version` `1.4.0` and `installed_version` `0.3.0`, and the line is `acme/greeter replaced by me/greeter 0.3.0 (as acme/greeter 1.4.0) for: …`
