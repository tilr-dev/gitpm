## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace`, `replace-check`, `replace-resolve` (fork-replacement); `auth check` (git-safety); `catalog show`, `catalog update`, `catalog register`, `check` (catalog); `catalog links` (below); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below)

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog links`
- **THEN** it prints the JSON links of the catalog root level ("JSON output for state and catalog")

#### Scenario: Install forms
- **WHEN** the user runs `install SOURCE [ID] [--package ID] [--ref REF | --dev] [--force]`, or `download` with the same arguments except `--force`
- **THEN** the positional `ID` and `--package` select a package as in multi-package-repos, and `--ref`/`--dev`/`--force` behave as in install-uninstall

#### Scenario: Update forms
- **WHEN** the user runs `update ID [--ref REF | --dev | --release] [--detach]` or `update all [--dry-run]`
- **THEN** the behavior is the one in updates; `--channel` is an unknown option

#### Scenario: Every command documented
- **WHEN** the unit test `tests::every_command_has_a_usage_block` runs
- **THEN** each command of the CLI has a usage block

#### Scenario: Removed command
- **WHEN** the user runs a command that is not in the list, including the former `create`, `migrate-catalog`, `info`, `catalog-update` and `register`
- **THEN** it fails with `unknown command: X; use --help`

#### Scenario: Replacement forms
- **WHEN** a caller uses `replace`, `replace-check` or `replace-resolve`
- **THEN** each takes `ORIGINAL --with PROVIDER`; `replace-check` prints a JSON report without mutations and `replace-resolve` explicitly installs missing dependencies; `replace --back` and `replace --force` are rejected

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `replace-resolve`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (scenario "Single-package verbs"), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `fork`, `commit`, `catalog update` or `catalog register`
- **THEN** they print the output specified in their capability (authoring, fork-replacement, catalog); this rule does not apply to them

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`, the second line taken from the plan's `unchanged` list (updates)
- **AND** a kept-back package prints `c kept back at V (latest L: REASON)` with the message of the plan's `HeldBack.reason`, and a broken consumer `d skipped at V (dependency DEP is not installed)` from its `unchanged` reason
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`, `forget_says_whether_the_clone_was_kept` and `update_all_reports_kept_back_packages_and_replace_with_original_without_a_replacement`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `acme/greeter`
- **THEN** the lines are `installed acme/greeter@V from SOURCE` (or `acme/greeter already installed (V)`, optionally with `; run update`), `acme/greeter updated A → B` (or `acme/greeter up to date (V)`), `acme/greeter rolled back B → A`, `acme/greeter enabled`, `acme/greeter disabled`, `acme/greeter forgotten (clone kept; restore re-registers it)` (or, when `ForgetResult.clone_kept` is false, `acme/greeter forgotten (no clone; record and dist removed)`), `acme/greeter restored V` and `acme/greeter uninstalled`
- **AND** `download` prints `downloaded acme/greeter@V from SOURCE (disabled)`; install and download print one such line per package, including each dependency pulled from the catalog
- **AND** an `update` or `update all` line of a change marked `major` ends with ` (major)` (`acme/greeter updated 1.2.3 → 2.0.0 (major)`; updates, "Major updates are marked"), covered by `update_lines_mark_major_updates`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces a package or returns through replace ORIGINAL --with ORIGINAL
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring

### Requirement: Prompts only on a TTY
The CLI SHALL prompt (`[y/N]`, on stderr) only when stdin is a terminal for uninstall with lost work. Without a TTY and unless `--force`, it SHALL call the library unconfirmed, which refuses as install-uninstall specifies. A declined prompt is a CLI error without `Error kind` (exit status 1). Library calls never prompt. Replacement SHALL never prompt.

#### Scenario: Uninstall prompt stream
- **WHEN** uninstall asks `Delete anyway? [y/N]`
- **THEN** the question is written to stderr

#### Scenario: Replacement prompt declined
- **WHEN** a replacement is inconsistent and terminal input contains n
- **THEN** the CLI returns the library error without asking for or reading confirmation

## ADDED Requirements

### Requirement: Replacement dependency conflict guidance
A failed `replace ORIGINAL --with PROVIDER` with a dependency conflict SHALL suggest explicit replacement checking and resolution using the supplied original and provider IDs.

#### Scenario: Missing dependencies with a configured catalog
- **GIVEN** a catalog is configured and the replacement provider needs an absent dependency available in that catalog
- **WHEN** the user runs `replace ORIGINAL --with PROVIDER`
- **THEN** it refuses without changing the workspace, names the consumer, dependency requirement and selected missing versions without claiming that no catalog is configured
- **AND** the hint points to `gitpm replace-check ORIGINAL --with PROVIDER` and `gitpm replace-resolve ORIGINAL --with PROVIDER`

#### Scenario: An installed dependency cannot be resolved by installing missing releases
- **GIVEN** the replacement requires an incompatible or disabled installed dependency
- **WHEN** `replace ORIGINAL --with PROVIDER` reports a dependency conflict
- **THEN** its hint describes checking dependency issues and installing missing dependencies only when resolvable
- **AND** it tells the caller to fix incompatible or disabled installed dependencies first
