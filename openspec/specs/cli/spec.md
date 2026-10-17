# cli Specification

## Purpose

Defines the `gitpm` command-line client (crate `gitpm-cli`): commands, settings resolution, output lines, help and version text, prompts, warnings and exit codes. The CLI only presents the library; it holds no business logic.

## Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of `COMMANDS` in `main.rs`; each one's behavior is specified in the named capability: `init`/`create`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `info`, `catalog`, `catalog-update`, `migrate-catalog`, `register`, `check` (catalog); `install`, `install-catalog`, `enable`, `disable`, `forget`, `restore`, `uninstall` (install-uninstall); `update`, `rollback` (updates); `list` (below); `sync-dist` (dist).

#### Scenario: Install forms
- **WHEN** the user runs `install SOURCE [ID] [--package ID] [--ref REF | --channel stable|dev]`
- **THEN** the positional `ID` and `--package` select a package as in multi-package-repos, and `--ref`/`--channel` behave as in install-uninstall

#### Scenario: Update forms
- **WHEN** the user runs `update ID [--ref REF | --channel stable|dev]` or `update all [--dry-run]`
- **THEN** the behavior is the one in updates

#### Scenario: Every command documented
- **WHEN** the unit test `tests::every_command_has_a_usage_block` runs
- **THEN** each command in `COMMANDS` has a usage block

#### Scenario: Removed command
- **WHEN** the user runs a command that is not in the list
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: Settings come from flags, environment, defaults
The CLI SHALL resolve `--root DIR` / `GITPM_ROOT` (default `./gitpm-workspace`), `--catalog LOC` / `GITPM_CATALOG` (no default) and `--sources FILE` / `GITPM_SOURCES` (default `./sources.txt`), flag first. There SHALL be no config file, and the old `PM_*` variables SHALL be ignored. Verified by `gitpm_variables_set_root_catalog_and_sources`, `legacy_pm_variables_are_ignored`, `defaults_are_relative_to_the_current_directory`.

#### Scenario: Legacy variable
- **WHEN** `PM_ROOT=/elsewhere` is set and `GITPM_ROOT` is not
- **THEN** the workspace is `./gitpm-workspace`

### Requirement: Commands that need a catalog say so
Catalog commands (`info`, `catalog`, `catalog-update`, `migrate-catalog`, `register`), `install-catalog` and `check` on a non-empty workspace SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` and create nothing. For a direct `install`, a local catalog path that does not exist SHALL count as no catalog. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: check without a catalog
- **WHEN** plugins are installed and the user runs `gitpm check` without a catalog
- **THEN** it fails with `check needs a catalog: …`

### Requirement: Option validation
The CLI SHALL refuse unknown options, repeated options, options given to commands they do not apply to (for example `--force` outside uninstall/disable/forget/replace/`fork --replace`, `--dry-run` outside `update all`/`sync-dist`, `--version` outside `install-catalog`/`fork --replace`), invalid combinations and wrong argument counts, with a message ending `; use --help` where applicable, before opening the workspace. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm update ID --channel dev --ref x`, or `--channel` with a value other than `stable`/`dev`
- **THEN** it fails with `--channel conflicts with --ref` (or `channel must be stable or dev`) and creates nothing

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/forget/replace/fork --replace`

### Requirement: One output line per affected package
Every mutating command SHALL print one line per affected package in the form `ID <verb> …` (detail lines such as `Fork created:` aside) and SHALL NOT print `OK:` lines. The line formats are the table in README "Changed in 0.5"; the scenarios below pin the common ones.

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `acme/greeter`
- **THEN** the lines are `installed acme/greeter@V from SOURCE` (or `acme/greeter already installed (V)`, optionally with `; run update`), `acme/greeter updated A → B` (or `acme/greeter up to date (V)`), `acme/greeter rolled back B → A`, `acme/greeter enabled`, `acme/greeter disabled`, `acme/greeter forgotten (clone kept; restore re-registers it)`, `acme/greeter restored V` and `acme/greeter uninstalled`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user forks, replaces and commits
- **THEN** the lines are `FORK forked from ORIGINAL (FOLDER)`, `ORIGINAL replaced by FORK V [(as ORIGINAL V)] for: consumers`, `ORIGINAL switched back from FORK (…)` and `ID committed V (local commit and tag TAG)` (exact variants in fork-replacement)

### Requirement: JSON output for state and catalog
`list` SHALL print the raw state plus `resolved` as pretty JSON. Each `resolved[]` entry SHALL always have `id`, `source`, `current`, `previous`, `enabled`, `active`, `title`, `description`, `provides`, `tags` (absent strings `null`, absent lists `[]`) and SHALL omit `forkOf`, `replaces`, `catalog`, `path`, `clone`, `dependencies`, `replacing`, `dependents` when empty. `catalog` and `register` print JSON. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Filter by service
- **WHEN** the user runs `gitpm list --provides theme`
- **THEN** `plugins` and `resolved` are filtered as specified in plugin-metadata ("Filter by provided service")

#### Scenario: Absent optional fields
- **WHEN** a package has no fork, replacement, catalog, folder, shared clone or dependencies
- **THEN** its `resolved[]` entry has none of those keys and `previous` is `null` before the first update

#### Scenario: list
- **WHEN** one package is installed
- **THEN** `gitpm list` prints a JSON object with `plugins` and a `resolved` array of one entry

### Requirement: Warnings go to stderr
Warnings SHALL be printed to stderr as `Warning: …` lines (ignored metadata keys, skipped release tags, unmanaged `dist/` paths, undeployed packages), and after every mutating command the CLI SHALL report `dist/` paths left alone. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Skipped tag on install
- **WHEN** an install skips a broken historical tag
- **THEN** stderr contains `Warning: skipped release tag NAME: reason`

### Requirement: Errors and exit codes
On failure the CLI SHALL print `Error: MESSAGE` to stderr, for a library error also `Error kind: KIND` (usage errors detected by the CLI itself, such as unknown options or `sync-dist` leaving packages undeployed, print no kind) (and for `local_changes` one `  - item` line per lost item), and exit with status 1; on success it SHALL exit 0. Verified by `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: Unknown ID
- **WHEN** the user runs `gitpm update acme/none`
- **THEN** stderr has `Error: plugin acme/none is not installed` and `Error kind: not_installed`, and the exit status is 1

### Requirement: Help
`gitpm --help`, `-h`, `help` or no arguments SHALL print the full usage; `gitpm COMMAND --help` (or `-h`) SHALL print only that command's usage block; an unknown command SHALL fail with `unknown command: X; use --help`. Every command SHALL have a usage block. Verified by `command_help_prints_only_that_command`, `help_names_the_binary_and_removed_commands_are_unknown` and `tests::every_command_has_a_usage_block`.

#### Scenario: Command help
- **WHEN** the user runs `gitpm init --help`
- **THEN** only the init/create usage block is printed and the exit status is 0

### Requirement: Version string
`gitpm --version` (or `-V`) SHALL print `gitpm VERSION`; a build from a Git checkout of this repository (one that tracks `crates/gitpm-cli/build.rs`) whose HEAD is not at a tag matching `v[0-9]*` SHALL print `gitpm VERSION-dev+SHORTHASH`. Packaged builds (crates.io, release binaries) SHALL print the plain version. Verified by `version_tests::version_starts_with_the_package_version` and the CI smoke test.

#### Scenario: Dev build
- **WHEN** the binary is built from `dev` at commit `0b146cf`
- **THEN** `gitpm --version` prints `gitpm 0.5.0-dev+0b146cf`

### Requirement: Prompts only on a TTY
The CLI SHALL prompt (`[y/N]`) only when stdin is a terminal, for uninstall with lost work and for incompatible replacements; without a TTY it SHALL refuse unless `--force`. Library calls SHALL never prompt.

#### Scenario: Replacement prompt declined
- **WHEN** a replacement is incompatible, stdin is a TTY and the user answers `n`
- **THEN** the CLI fails with `replacement cancelled; nothing was changed`

### Requirement: Nothing is built, executed or uploaded
The CLI SHALL only clone Git repositories, copy their `dist/` folders and write local catalogs; it SHALL never build, execute package code, push or upload.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog-update` against an HTTPS catalog location
- **THEN** the command fails with `invalid_argument` ("catalog-update requires a local output catalog"), because HTTP(S) catalogs are read-only
