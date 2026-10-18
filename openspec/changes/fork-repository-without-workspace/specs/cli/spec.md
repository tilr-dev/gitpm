## MODIFIED Requirements

### Requirement: Option validation
The CLI SHALL refuse unknown options, repeated options, options given to commands they do not apply to (for example `--force` outside uninstall/disable/forget/replace/`fork --replace`, `--dry-run` outside `update all`/`sync-dist`, `--from` outside `fork`, `--ref` with `fork` but without `--from`, `--replace` with `--from`), invalid combinations and wrong argument counts, with a message ending `; use --help` where applicable, before opening the workspace (CLI usage errors without `Error kind`). Verified by `usage_errors_name_the_problem_and_create_nothing`.

#### Scenario: Channel and ref
- **WHEN** the user runs `gitpm update ID --channel dev --ref x` or `gitpm install SOURCE --ref x --channel stable`, or `--channel` with a value other than `stable`/`dev`
- **THEN** it fails with `--channel conflicts with --ref` (or `channel must be stable or dev`) and creates nothing
- **AND** `--channel` of any value is refused together with `--ref`, and the exit status is 1; `--version` outside `install-catalog`/`fork --replace` is refused the same way

#### Scenario: Misplaced flag
- **WHEN** the user runs `gitpm list --force`
- **THEN** it fails with `--force is only valid with uninstall/disable/forget/replace/fork --replace`

#### Scenario: Fork from a source
- **WHEN** the user runs `gitpm fork tilr/theme --from ./tilr-plugins --ns me --into x --ref tilr/theme@0.1.0`
- **THEN** the options are accepted, while `gitpm fork tilr/theme --ns me --into x --ref y` fails naming `--ref` and creates nothing

#### Scenario: Flag position
- **WHEN** the user runs `gitpm uninstall ID --force` or `gitpm uninstall --force ID`
- **THEN** both behave the same: options may come before or after the positional arguments
