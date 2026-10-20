## ADDED Requirements

### Requirement: Catalog command group
The CLI SHALL provide the catalog operations as subcommands of `gitpm catalog`: `show` (text overview), `links` (JSON links of one level), `update [--sources FILE]` (generate from Git release tags) and `register GIT_SOURCE --ref REF [--package ID] [--build-url URL --build-sha256 SHA]` (one committed version). Each SHALL behave and print as its operation does under the `catalog` spec.

#### Scenario: Generate a catalog with the group name
- **WHEN** `sources.txt` lists `./greeter` with tags 0.1.0 and 0.2.0 and the user runs `gitpm --catalog ./catalog catalog update`
- **THEN** stdout is `Catalog ready: 2 release(s) checked from sources.` and the exit status is 0

#### Scenario: Register with the group name
- **WHEN** the user runs `gitpm --catalog ./catalog catalog register ./greeter --ref acme/greeter@0.1.0`
- **THEN** the release is added and printed as pretty JSON

### Requirement: Catalog subcommands need a catalog
Every `catalog` subcommand SHALL need a catalog location; without `--catalog` or `GITPM_CATALOG` it SHALL fail with `catalog SUB needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` and create nothing.

#### Scenario: Missing catalog names the subcommand
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm catalog show`
- **THEN** it fails with `catalog show needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` and no file is created

#### Scenario: Writing to an HTTP catalog
- **WHEN** the user runs `gitpm --catalog https://pkg.example/catalog.json catalog update`
- **THEN** the command fails with `invalid_argument`, because HTTP(S) catalogs are read-only; the library message names no command and the CLI adds the `gitpm catalog update` hint; nothing is uploaded

### Requirement: Catalog group usage and errors
`gitpm catalog --help` SHALL print the usage blocks of every catalog subcommand, and `gitpm catalog SUB --help` only that subcommand's block. Bare `gitpm catalog` and an unknown word after `catalog` SHALL fail before opening a workspace or catalog, as SHALL wrong argument counts or misplaced options, naming the two-word command (for example `--sources requires catalog update`).

#### Scenario: Unknown subcommand
- **WHEN** the user runs `gitpm catalog publish`
- **THEN** it fails with `unknown catalog command: publish; use gitpm catalog --help`, exit status 1, and nothing is created

#### Scenario: Bare catalog
- **WHEN** the user runs `gitpm --catalog ./catalog catalog`
- **THEN** it fails with `catalog needs a subcommand: show, links, update or register; use gitpm catalog --help` and exit status 1

#### Scenario: Subcommand help
- **WHEN** the user runs `gitpm catalog register --help`
- **THEN** only the `catalog register` usage block is printed and the exit status is 0

#### Scenario: Misplaced sources flag
- **WHEN** the user runs `gitpm catalog show --sources list.txt`
- **THEN** it fails with `--sources requires catalog update`

## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `catalog show`, `catalog update`, `catalog register`, `check` (catalog); `catalog links` (below); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below)

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

### Requirement: Commands that need a catalog say so
Catalog commands (`catalog show`, `catalog links`, `catalog update`, `catalog register`) and `install-catalog` SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`) and create nothing; `check` works without one. A given location SHALL be passed to the library unchanged, also for `install`. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: Catalog command without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm catalog show`
- **THEN** it fails with `catalog show needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`, exit status 1, and no file is created
- **AND** which operations need a catalog is specified in catalog ("A catalog is optional")

#### Scenario: Catalog path that does not exist
- **WHEN** the user runs `gitpm --catalog ./missing install ./greeter`
- **THEN** it fails with the library's `invalid_argument` (catalog, "A configured catalog location must exist"), prints `Error kind: invalid_argument`, exits with status 1 and creates nothing

#### Scenario: check without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm check` on a workspace with packages
- **THEN** it prints the check table from each package's origin and exits 0

### Requirement: Option validation
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, misplaced options (for example `--dry-run` outside `update all`/`sync-dist`), exclusive options (`--ref`, `--dev`, `--release`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`). Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Ref, dev and release
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev`, `gitpm update ID --dev --release`, or passes `--channel`
- **THEN** it fails with `--dev conflicts with --ref` (or `--release conflicts with --dev`), or `unknown option: --channel; use --help`, exit status 1, and creates nothing
- **AND** `--version` outside `install-catalog`/`download-catalog`/`fork --replace` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/enable/install/install-catalog/forget/replace/fork --replace`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `fork --replace`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (scenario "Single-package verbs"), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `fork`, `commit`, `catalog update` or `catalog register`
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
- **AND** an `update` or `update all` line of a change marked `major` ends with ` (major)` (`acme/greeter updated 1.2.3 → 2.0.0 (major)`; updates, "Major updates are marked"), covered by `update_lines_mark_major_updates`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces, switches back or runs `fork --replace`
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring

### Requirement: JSON output for state and catalog
`list` SHALL print the library's list view (`list_with`, raw state plus `resolved`) as pretty JSON; each `resolved[]` entry SHALL always have the fields of plugin-metadata ("Reported shape") and omit the empty optional ones (`catalog` is set only for protected packages). `catalog links` SHALL print the library's `CatalogLinks` (`{schema, directories}`, as FFI `catalogLinks`), `catalog register` JSON. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

#### Scenario: Filter by service
- **WHEN** the user runs `gitpm list --provides theme`
- **THEN** the library filters `packages` and `resolved` as specified in plugin-metadata ("Filter by provided service"); the CLI only prints the result

#### Scenario: Absent optional fields
- **WHEN** a package has no fork, replacement, catalog, folder, shared clone or dependencies
- **THEN** its `resolved[]` entry always has `id`, `source`, `current`, `previous`, `enabled`, `active`, `title`, `description`, `provides`, `tags`, has none of `forkOf`, `replaces`, `catalog`, `path`, `clone`, `dependencies`, `replacing`, `dependents` and `previous` is `null` before the first update

#### Scenario: list
- **WHEN** one package is installed
- **THEN** `gitpm list` prints a JSON object with `packages` and a `resolved` array of one entry, without locking or creating the workspace (transactions-locking, "Workspace lock")

#### Scenario: Protected package in list
- **WHEN** a package is protected by a verified catalog
- **THEN** its `resolved[]` entry has `catalog` (only protected packages have it) and `list` shows nothing else about catalogs

### Requirement: Help
`gitpm --help`, `-h`, `help` or no arguments SHALL print the full usage; `gitpm COMMAND --help` (or `-h`) SHALL print only that command's usage block; an unknown command SHALL fail with `unknown command: X; use --help`. Every command SHALL have a usage block. Verified by `command_help_prints_only_that_command`, `help_names_the_binary_and_removed_commands_are_unknown` and `tests::every_command_has_a_usage_block`.

#### Scenario: Command help
- **WHEN** the user runs `gitpm init --help`
- **THEN** only the init usage block is printed and the exit status is 0
