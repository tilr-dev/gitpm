## MODIFIED Requirements

### Requirement: Commands that need a catalog say so
Catalog commands (`info`, `catalog`, `catalog-update`, `register`), `install-catalog` and `check` on a non-empty workspace SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`) and create nothing. A given location SHALL be passed to the library unchanged, also for `install`. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: Catalog command without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm info`
- **THEN** it fails with `info needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`, exit status 1, and no file is created
- **AND** which operations need a catalog is specified in catalog ("A catalog is optional")

#### Scenario: Catalog path that does not exist
- **WHEN** the user runs `gitpm --catalog ./missing install ./greeter`
- **THEN** it fails with the library's `invalid_argument` (catalog, "A configured catalog location must exist"), prints `Error kind: invalid_argument`, exits with status 1 and creates nothing

### Requirement: Option validation
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, options on commands they do not apply to (for example `--dry-run` outside `update all`/`sync-dist`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`). Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm install SOURCE --ref x --channel dev`
- **THEN** the CLI passes both and prints the library's refusal (`invalid_argument`, install-uninstall), exit status 1; `--channel` with a value other than `stable`/`dev` is a CLI usage error (`channel must be stable or dev`)
- **AND** `--version` outside `install-catalog`/`fork --replace` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/forget/replace/fork --replace`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `fork --replace`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (the install line is `installed ID@V from SOURCE`), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

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

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces, switches back or runs `fork --replace`
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring

### Requirement: JSON output for state and catalog
`list` SHALL print the library's list view (`list_with`, raw state plus `resolved`) as pretty JSON. Each `resolved[]` entry SHALL always have `id`, `source`, `current`, `previous`, `enabled`, `active`, `title`, `description`, `provides`, `tags` (plugin-metadata) and SHALL omit `forkOf`, `replaces`, `catalog`, `path`, `clone`, `dependencies`, `replacing`, `dependents` when empty. `catalog` and `register` print JSON. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Filter by service
- **WHEN** the user runs `gitpm list --provides theme`
- **THEN** the library filters `plugins` and `resolved` as specified in plugin-metadata ("Filter by provided service"); the CLI only prints the result

#### Scenario: Absent optional fields
- **WHEN** a package has no fork, replacement, catalog, folder, shared clone or dependencies
- **THEN** its `resolved[]` entry has none of those keys and `previous` is `null` before the first update

#### Scenario: list
- **WHEN** one package is installed
- **THEN** `gitpm list` prints a JSON object with `plugins` and a `resolved` array of one entry, without locking or creating the workspace (transactions-locking, "Workspace lock")

### Requirement: Warnings go to stderr
The CLI SHALL print every warning a library result carries (ignored metadata keys, skipped release tags, and the dist report's `unmanaged` paths and `skipped` packages, dist "Mutating results carry the dist report") to stderr as `Warning: …` lines; it SHALL NOT derive warnings from anything but the result. Verified by `any_mutating_command_reconciles_the_whole_dist`.

#### Scenario: Skipped tag on install
- **WHEN** an install skips a broken historical tag
- **THEN** stderr contains `Warning: skipped release tag NAME: reason`, taken from `InstallResult.skipped`

### Requirement: Errors and exit codes
On failure the CLI SHALL print `Error: MESSAGE` to stderr, for a library error also `Error kind: KIND` (and for `local_changes` one `  - item` line per lost item) and, for kinds where a gitpm command helps, one `Hint: …` line naming that command; CLI usage errors print no kind. It SHALL exit with status 1 on failure and 0 on success. Verified by `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: Unknown ID
- **WHEN** the user runs `gitpm update acme/none`
- **THEN** stderr has `Error: plugin acme/none is not installed` and `Error kind: not_installed`, and the exit status is 1

#### Scenario: Command hint
- **WHEN** the user installs `acme/greeter` again after forgetting it while its clone is retained
- **THEN** stderr has the neutral library message, `Error kind: already_installed` and a `Hint:` line naming `gitpm restore acme/greeter` and `gitpm uninstall acme/greeter`

### Requirement: Prompts only on a TTY
The CLI SHALL prompt (`[y/N]`, on stderr) only when stdin is a terminal, for uninstall with lost work and for incompatible replacements; without a TTY, and unless `--force`, it SHALL call the library unconfirmed, which refuses as install-uninstall and fork-replacement specify. A declined prompt is a CLI error without `Error kind` (exit status 1). Library calls never prompt (git-safety, "One subprocess boundary").

#### Scenario: Replacement prompt declined
- **WHEN** a replacement is incompatible, stdin is a TTY and the user answers `n`
- **THEN** the CLI fails with `replacement cancelled; nothing was changed`

#### Scenario: Uninstall prompt stream
- **WHEN** an uninstall with lost work asks `Delete anyway? [y/N]`
- **THEN** the question is written to stderr, so stdout carries only result lines

## REMOVED Requirements

### Requirement: Nothing is built, executed or uploaded
**Reason**: A library guarantee, not CLI behavior (thin CLI, TILR-33); moved to catalog "Nothing is built, executed or uploaded".
**Migration**: None; the behavior is unchanged.
