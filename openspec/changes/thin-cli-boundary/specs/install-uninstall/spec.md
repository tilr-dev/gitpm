## MODIFIED Requirements

### Requirement: Install an explicit ref or the dev channel
`install SOURCE --ref REF` SHALL install the given tag, branch or commit; `--channel dev` SHALL follow the default branch HEAD and `--channel stable` SHALL mean the default. Library rule: an explicit ref together with the dev channel SHALL be refused with `invalid_argument`. A ref starting with `-` is refused as in git-safety ("No option injection"). Verified by `workspace::install_channels_and_explicit_refs` and `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`.

#### Scenario: Ref and dev channel in the library
- **WHEN** a caller passes a ref and the dev channel to `install_package`, or the user runs `gitpm install SOURCE --ref x --channel dev`
- **THEN** it fails with `invalid_argument` ("the dev channel conflicts with an explicit ref"); the CLI does not check the combination itself, and `--channel stable --ref x` installs `x`

#### Scenario: Install by exact commit
- **WHEN** the user runs `install SOURCE --ref <40-hex commit>`
- **THEN** that commit is installed and recorded as `current.commit`

### Requirement: Same ID from another source is refused
Installing an ID that is installed from a different source (including a `file://` URL against a plain path, or a URL with and without `.git`) SHALL fail with `already_installed`. Installing an ID that is forgotten but retained SHALL be refused with `already_installed` until it is restored or uninstalled. Verified by `forgotten_repository_blocks_reinstall_until_restored`.

#### Scenario: Other source
- **WHEN** `acme/greeter` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

#### Scenario: Forgotten but retained
- **WHEN** `acme/greeter` was forgotten and its clone `plugins/acme/greeter` is retained, and the user installs it again
- **THEN** the command fails with `already_installed` ("acme/greeter was forgotten and its clone is kept") and the workspace is unchanged
- **AND** the message names no command; the CLI adds a `Hint:` line naming `gitpm restore ID` and `gitpm uninstall ID` (the same holds for a forgotten package of a shared clone)

### Requirement: Refused input creates nothing
Input SHALL be validated before anything is created: the CLI checks argv syntax and parses IDs and versions with the library's parsers before opening, and `open_with` validates Options first (workspace-layout). A refused install SHALL leave no clone, no `dist/<id>`, no state entry and no `.pm-txn` (errors). Verified by `usage_errors_name_the_problem_and_create_nothing` and `workspace::install_refusals_leave_the_workspace_unchanged`.

#### Scenario: Invalid ID on a fresh root
- **WHEN** the user runs `gitpm --root new-ws update ../evil`
- **THEN** the command fails with `invalid_id` and `new-ws` is not created

### Requirement: CLI confirmation flow for uninstall
Without `--force`, the CLI SHALL list what `uninstall_plan` reports and ask `Delete anyway? [y/N]` on stderr on a TTY; anything but `y`/`yes` (EOF included) SHALL change nothing and fail with `uninstall cancelled; nothing was changed`. Without a TTY it calls the library unconfirmed, which refuses. `--force` deletes without asking and ignores dependents. Verified by `uninstall_on_tty_asks_before_losing_work` and `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and the exit status is 1
- **AND** a declined prompt is a CLI error without `Error kind` (exit status 1), and `--force` sets both `confirm` and `force`
