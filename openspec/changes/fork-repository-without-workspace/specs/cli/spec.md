## MODIFIED Requirements

### Requirement: Option validation
Before calling the library the CLI SHALL refuse syntax errors: unknown, repeated or misplaced options, conflicting options, values outside a fixed set and wrong argument counts. Usage errors SHALL omit `Error kind` and end with `; use --help` where applicable. The library SHALL check domain combinations. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Ref, dev and release
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev`, `gitpm update ID --dev --release`, or passes `--channel`
- **THEN** it fails with `--dev conflicts with --ref` (or `--release conflicts with --dev`), or `unknown option: --channel; use --help`, exit status 1, and creates nothing
- **AND** `--version` outside `install-catalog`/`download-catalog` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/enable/install/install-catalog/forget/replace`

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments

#### Scenario: Removed fork options have no side effects
- **WHEN** `fork` receives `--replace`, `--version V` or `--force`
- **THEN** it fails as a usage error before opening the workspace or creating the destination

#### Scenario: Fork from source options
- **WHEN** `fork ID --from SOURCE --ns NS --into DIR --ref REF` is run
- **THEN** the options are accepted without opening a workspace; `--ref` without `--from` is refused before side effects
- **AND** `--from` outside `fork`, and `--dry-run` outside `update all`/`sync-dist`, are refused as misplaced options
