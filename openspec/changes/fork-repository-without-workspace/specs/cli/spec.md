## MODIFIED Requirements

### Requirement: Option validation
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, misplaced options (for example `--dry-run` outside `update all`/`sync-dist`, `--sources` outside `catalog update`, `--from` outside `fork`, `--ref` with `fork` but without `--from`), exclusive options (`--ref`, `--dev`, `--release`; `--replace` with `--from`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`); a message about a catalog subcommand names the two-word command. Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev` or `gitpm update ID --dev --release`
- **THEN** it fails with `--dev conflicts with --ref` (or `--release conflicts with --dev`), exit status 1, and creates nothing
- **AND** `--version` outside `install-catalog`/`download-catalog`/`fork --replace` is refused by the CLI as a misplaced option

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force` or `gitpm catalog show --sources list.txt`
- **THEN** it fails with `--force is only valid with uninstall/disable/enable/forget/replace/fork --replace`, or `--sources requires catalog update`

#### Scenario: Fork from a source
- **WHEN** the user runs `gitpm fork tilr/theme --from ./tilr-plugins --ns me --into x --ref tilr/theme@0.1.0`
- **THEN** the options are accepted, while `gitpm fork tilr/theme --ns me --into x --ref y` fails naming `--ref` (not valid for `fork` without `--from`) and `gitpm fork tilr/theme --from ./tilr-plugins --ns me --into x --replace` fails naming `--replace`; both create nothing

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments
