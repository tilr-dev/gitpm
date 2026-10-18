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
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the named capability: `init`/`create`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `catalog show|links|update|register` (below, operations in catalog), `migrate-catalog`, `check` (catalog); `install`, `install-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below).

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog links`
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
- **WHEN** the user runs a command that is not in the list, including the former `info`, `catalog-update` and `register`
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: Commands that need a catalog say so
The `catalog` subcommands, `migrate-catalog`, `install-catalog` and `check` on a non-empty workspace SHALL fail without a catalog with `COMMAND needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG` (a CLI usage error without `Error kind`; COMMAND is the two-word name for a subcommand) and create nothing. For a direct `install`, a local catalog path that does not exist SHALL count as no catalog. Verified by `catalog_commands_name_the_missing_catalog_and_create_nothing`.

#### Scenario: Catalog command without a catalog
- **WHEN** neither `--catalog` nor `GITPM_CATALOG` is set and the user runs `gitpm catalog show`
- **THEN** it fails with `catalog show needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`, exit status 1, and no file is created
- **AND** which operations need a catalog is specified in catalog ("A catalog is optional")

### Requirement: Option validation
The CLI SHALL refuse unknown options, repeated options, options given to commands they do not apply to (for example `--force` outside uninstall/disable/forget/replace/`fork --replace`, `--dry-run` outside `update all`/`sync-dist`, `--sources` outside `catalog update`), invalid combinations and wrong argument counts, with a message ending `; use --help` where applicable, before opening the workspace (CLI usage errors without `Error kind`). Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm update ID --channel dev --ref x` or `gitpm install SOURCE --ref x --channel stable`, or `--channel` with a value other than `stable`/`dev`
- **THEN** it fails with `--channel conflicts with --ref` (or `channel must be stable or dev`) and creates nothing
- **AND** `--channel` of any value is refused together with `--ref`, and the exit status is 1; `--version` outside `install-catalog`/`fork --replace` is refused the same way

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/forget/replace/fork --replace`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

### Requirement: Help
`gitpm --help`, `-h`, `help` or no arguments SHALL print the full usage, with the catalog subcommands in a `Catalog` section; `gitpm COMMAND --help` (or `-h`) SHALL print only that command's usage block, and `gitpm catalog SUB --help` only that subcommand's; an unknown command SHALL fail with `unknown command: X; use --help`. Every command SHALL have a usage block. Verified by `command_help_prints_only_that_command`, `help_names_the_binary_and_removed_commands_are_unknown` and `tests::every_command_has_a_usage_block`.

#### Scenario: Command help
- **WHEN** the user runs `gitpm init --help`
- **THEN** only the init/create usage block is printed and the exit status is 0

### Requirement: Nothing is built, executed or uploaded
The CLI SHALL only clone Git repositories, copy their package dist folders, write local catalogs and, for the author commands (`init`, `commit`, `remote`, `fork`), create local repositories, commits, tags and remote settings; it SHALL never build, execute package code, push or upload.

#### Scenario: Catalog publishing
- **WHEN** the user runs `catalog update` against an HTTPS catalog location
- **THEN** the command fails with `invalid_argument`, because HTTP(S) catalogs are read-only, and nothing is uploaded
