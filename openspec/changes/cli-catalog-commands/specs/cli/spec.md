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
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `catalog show`, `catalog links`, `catalog update`, `catalog register` ("Catalog command group"; operations in catalog), `check` (catalog); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` ("JSON output for state and catalog")

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog links`
- **THEN** it prints the JSON links of the catalog root level ("JSON output for state and catalog")

#### Scenario: Install forms
- **WHEN** the user runs `install SOURCE [ID] [--package ID] [--ref REF | --dev]`, or `download` with the same arguments
- **THEN** the positional `ID` and `--package` select a package as in multi-package-repos, and `--ref`/`--dev` behave as in install-uninstall

#### Scenario: Update forms
- **WHEN** the user runs `update ID [--ref REF | --dev | --release] [--detach]` or `update all [--dry-run]`
- **THEN** the behavior is the one in updates

#### Scenario: Every command documented
- **WHEN** the unit test `tests::every_command_has_a_usage_block` runs
- **THEN** each command of the CLI has a usage block

#### Scenario: Removed command
- **WHEN** the user runs a command that is not in the list, including the former `info`, `catalog-update` and `register`
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: Commands that need a catalog say so
The `catalog` subcommands, `install-catalog` and `download-catalog` SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`; COMMAND is the two-word name for a subcommand) and create nothing; `check` works without one. A given location SHALL be passed to the library unchanged, also for `install`. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

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
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, misplaced options (for example `--dry-run` outside `update all`/`sync-dist`, `--sources` outside `catalog update`), exclusive options (`--ref`, `--dev`, `--release`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`); a message about a catalog subcommand names the two-word command. Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev` or `gitpm update ID --dev --release`
- **THEN** it fails with `--dev conflicts with --ref` (or `--release conflicts with --dev`), exit status 1, and creates nothing
- **AND** `--version` outside `install-catalog`/`download-catalog`/`fork --replace` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force` or `gitpm catalog show --sources list.txt`
- **THEN** it fails with `--force is only valid with uninstall/disable/enable/forget/replace/fork --replace`, or `--sources requires catalog update`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

### Requirement: Help
`gitpm --help`, `-h`, `help` or no arguments SHALL print the full usage, with the catalog subcommands in a `Catalog` section; `gitpm COMMAND --help` (or `-h`) SHALL print only that command's usage block, and `gitpm catalog SUB --help` only that subcommand's; an unknown command SHALL fail with `unknown command: X; use --help`. Every command SHALL have a usage block. Verified by `command_help_prints_only_that_command`, `help_names_the_binary_and_removed_commands_are_unknown` and `tests::every_command_has_a_usage_block`.

#### Scenario: Command help
- **WHEN** the user runs `gitpm init --help`
- **THEN** only the init usage block is printed and the exit status is 0
