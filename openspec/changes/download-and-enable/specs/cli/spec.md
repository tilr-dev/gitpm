## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `info`, `catalog-update`, `register`, `check` (catalog); `catalog` ("JSON output for state and catalog"); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` ("JSON output for state and catalog")

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog`
- **THEN** it prints the JSON links of the catalog root level ("JSON output for state and catalog")

#### Scenario: Install forms
- **WHEN** the user runs `install SOURCE [ID] [--package ID] [--ref REF | --dev]`, or `download` with the same arguments
- **THEN** the positional `ID` and `--package` select a package as in multi-package-repos, and `--ref`/`--dev` behave as in install-uninstall

#### Scenario: Update forms
- **WHEN** the user runs `update ID [--ref REF | --dev | --release]` or `update all [--dry-run]`
- **THEN** the behavior is the one in updates; `--channel` is an unknown option

#### Scenario: Every command documented
- **WHEN** the unit test `tests::every_command_has_a_usage_block` runs
- **THEN** each command of the CLI has a usage block

#### Scenario: Removed command
- **WHEN** the user runs a command that is not in the list, including the former `create` and `migrate-catalog`
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: Option validation
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, misplaced options (for example `--dry-run` outside `update all`/`sync-dist`), exclusive options (`--ref`, `--dev`, `--release`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`). Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev`, `gitpm update ID --dev --release`, or passes `--channel`
- **THEN** it fails with `--dev conflicts with --ref` (or `--release conflicts with --dev`), or `unknown option --channel; use --help`, exit status 1, and creates nothing
- **AND** `--version` outside `install-catalog`/`download-catalog`/`fork --replace` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/enable/forget/replace/fork --replace`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `fork --replace`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (install lines are `installed ID@V from SOURCE`), built from the library result alone, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `fork`, `commit`, `catalog-update` or `register`
- **THEN** they print the output specified in their capability (authoring, fork-replacement, catalog); this rule does not apply to them

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`, the second line taken from the plan's `unchanged` list (updates)
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `acme/greeter`
- **THEN** the lines are `installed acme/greeter@V from SOURCE` (or `acme/greeter already installed (V)`, optionally with `; run update`), `acme/greeter updated A → B` (or `acme/greeter up to date (V)`), `acme/greeter rolled back B → A`, `acme/greeter enabled`, `acme/greeter disabled`, `acme/greeter forgotten (clone kept; restore re-registers it)`, `acme/greeter restored V` and `acme/greeter uninstalled`
- **AND** `download` prints `downloaded acme/greeter@V from SOURCE (disabled)`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces, switches back or runs `fork --replace`
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring
