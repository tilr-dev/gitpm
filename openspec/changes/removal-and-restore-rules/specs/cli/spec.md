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

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `fork --replace`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (scenario "Single-package verbs"), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `fork`, `commit`, `catalog-update` or `register`
- **THEN** they print the output specified in their capability (authoring, fork-replacement, catalog); this rule does not apply to them

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`, the second line taken from the plan's `unchanged` list (updates)
- **AND** a kept-back package prints `c kept back at V (latest L: REASON)` with the message of the plan's `HeldBack.reason`, and a broken consumer `d skipped at V (dependency DEP is not installed)` from its `unchanged` reason
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`, `forget_says_whether_the_clone_was_kept` and `update_all_reports_kept_back_packages_and_replace_back_without_a_replacement`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `acme/greeter`
- **THEN** the lines are `installed acme/greeter@V from SOURCE` (or `acme/greeter already installed (V)`, optionally with `; run update`), `acme/greeter updated A → B` (or `acme/greeter up to date (V)`), `acme/greeter rolled back B → A`, `acme/greeter enabled`, `acme/greeter disabled`, `acme/greeter forgotten (clone kept; restore re-registers it)` (or, when `ForgetResult.clone_kept` is false, `acme/greeter forgotten (no clone; record and dist removed)`), `acme/greeter restored V` and `acme/greeter uninstalled`
- **AND** `download` prints `downloaded acme/greeter@V from SOURCE (disabled)`; install and download print one such line per package, including each dependency pulled from the catalog

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces, switches back or runs `fork --replace`
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring
