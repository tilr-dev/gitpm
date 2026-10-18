## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace` (fork-replacement); `auth check` (git-safety); `info`, `catalog-update`, `register`, `check` (catalog); `catalog` (below); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below)

#### Scenario: Catalog links
- **WHEN** the user runs `gitpm --catalog ./catalog catalog`
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
- **WHEN** the user runs a command that is not in the list, including the former `create` and `migrate-catalog`
- **THEN** it fails with `unknown command: X; use --help`

### Requirement: JSON output for state and catalog
`list` SHALL print the library's list view (`list_with`, raw state plus `resolved`) as pretty JSON; each `resolved[]` entry SHALL always have the fields of plugin-metadata ("Reported shape") and omit the empty optional ones (scenario); `catalog` is present only for a package protected by a verified catalog. `catalog` SHALL print the library's `CatalogLinks` (`{schema, directories}`, as FFI `catalogLinks`), `register` JSON. Verified by `list_reports_metadata_present_or_absent_and_filters_by_provides`.

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
