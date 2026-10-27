## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace`, `replace-check`, `replace-resolve` (fork-replacement); `auth check` (git-safety); `catalog show`, `catalog update`, `catalog register`, `check` (catalog); `catalog links` (below); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below); `packages`, `dependency add`, `dependency update`, `dependency remove` (package-manifest)

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

#### Scenario: Identity forms
- **WHEN** a caller reads command usage
- **THEN** package selectors and replacement arguments are UUIDs, authoring uses `init NAME --into DIR`, and forking uses `fork UUID [UUID...] --into DIR` without `--ns`
