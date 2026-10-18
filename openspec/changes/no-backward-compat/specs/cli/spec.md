## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the named capability: `init`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `info`, `catalog-update`, `register`, `check` (catalog); `catalog` (below); `install`, `install-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below), and no aliases.

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog`
- **THEN** it prints the JSON links of the catalog root level ("JSON output for state and catalog")

#### Scenario: Install forms
- **WHEN** the user runs `install SOURCE [ID] [--package ID] [--ref REF | --channel stable|dev]`
- **THEN** the positional `ID` and `--package` select a package as in multi-package-repos, and `--ref`/`--channel` behave as in install-uninstall

#### Scenario: Update forms
- **WHEN** the user runs `update ID [--ref REF | --channel stable|dev]` or `update all [--dry-run]`
- **THEN** the behavior is the one in updates

#### Scenario: Every command documented
- **WHEN** the unit test `tests::every_command_has_a_usage_block` runs
- **THEN** each command of the CLI has a usage block

#### Scenario: Removed command
- **WHEN** the user runs a command that is not in the list, including the former `create` and `migrate-catalog`
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: Commands that need a catalog say so
Catalog commands (`info`, `catalog`, `catalog-update`, `register`), `install-catalog` and `check` on a non-empty workspace SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`) and create nothing. For a direct `install`, a local catalog path that does not exist SHALL count as no catalog. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: Catalog command without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm info`
- **THEN** it fails with `info needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`, exit status 1, and no file is created
- **AND** which operations need a catalog is specified in catalog ("A catalog is optional")

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `fork --replace`, `sync-dist`) and the author commands `fork` and `commit` SHALL print one line per affected package in the form `ID <verb> …` (detail lines such as `Fork created:` aside; the install line is `installed ID@V from SOURCE`) and SHALL NOT print `OK:` lines. The line formats are the table in README "Changed in 0.5".

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `catalog-update` or `register`
- **THEN** they print the output specified in their capability, not per-package lines

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `acme/greeter`
- **THEN** the lines are `installed acme/greeter@V from SOURCE` (or `acme/greeter already installed (V)`, optionally with `; run update`), `acme/greeter updated A → B` (or `acme/greeter up to date (V)`), `acme/greeter rolled back B → A`, `acme/greeter enabled`, `acme/greeter disabled`, `acme/greeter forgotten (clone kept; restore re-registers it)`, `acme/greeter restored V` and `acme/greeter uninstalled`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user forks, replaces and commits
- **THEN** the lines are `FORK forked from ORIGINAL (FOLDER)`, `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers`, `ORIGINAL switched back from FORK (…)` and `ID committed V (local commit and tag TAG)` (exact variants in fork-replacement)

### Requirement: Help
`gitpm --help`, `-h`, `help` or no arguments SHALL print the full usage; `gitpm COMMAND --help` (or `-h`) SHALL print only that command's usage block; an unknown command SHALL fail with `unknown command: X; use --help`. Every command SHALL have a usage block. Verified by `command_help_prints_only_that_command`, `help_names_the_binary_and_removed_commands_are_unknown` and `tests::every_command_has_a_usage_block`.

#### Scenario: Command help
- **WHEN** the user runs `gitpm init --help`
- **THEN** only the init usage block is printed and the exit status is 0
