## MODIFIED Requirements

### Requirement: Command set
The CLI SHALL offer exactly the commands of its full usage (`gitpm --help`), each specified in the capability named in the scenario "Command list", and no aliases.

#### Scenario: Command list
- **WHEN** the user reads `gitpm --help`
- **THEN** the commands are `init`, `commit`, `remote` (authoring); `fork`, `replace`, `replace-check`, `replace-resolve` (fork-replacement); `auth check` (git-safety); `catalog show`, `catalog update`, `catalog register`, `check` (catalog); `catalog links` (below); `install`, `install-catalog`, `download`, `download-catalog`, `forget`, `restore`, `uninstall` (install-uninstall); `enable`, `disable`, `sync-dist` (dist); `update`, `rollback` (updates); `list` (below)

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
- **WHEN** `fork` receives `--ns NAMESPACE`, `--replace`, `--version V` or `--force`
- **THEN** it fails as a usage error before opening the workspace or creating the destination

#### Scenario: Fork from source options
- **WHEN** `fork ID --from SOURCE --into DIR --ref REF` is run
- **THEN** the options are accepted without opening a workspace; `--ref` without `--from` is refused before side effects
- **AND** `--from` outside `fork`, and `--dry-run` outside `update all`/`sync-dist`, are refused as misplaced options

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `replace-resolve`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (scenario "Single-package verbs"), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

#### Scenario: Other author and catalog commands
- **WHEN** the user runs `init`, `remote`, `fork`, `commit`, `catalog update` or `catalog register`
- **THEN** they print the output specified in their capability (authoring, fork-replacement, catalog); this rule does not apply to them

#### Scenario: update all lines
- **WHEN** `update all` updates one package and finds another current
- **THEN** it prints `a updated 1.0.0 → 1.1.0`, `b up to date (2.0.0)` and `1 updates, 0 installs, 0 kept back.`, the second line taken from the plan's `unchanged` list (updates)
- **AND** a kept-back package prints `c kept back at V (latest L: REASON)` with the message of the plan's `HeldBack.reason`, and a broken consumer `d skipped at V (dependency DEP is not installed)` from its `unchanged` reason
- **AND** this is covered by `mutating_commands_print_one_line_per_package_and_no_ok_lines`, `forget_says_whether_the_clone_was_kept` and `update_all_reports_kept_back_packages_and_replace_with_original_without_a_replacement`

#### Scenario: Single-package verbs
- **WHEN** the user installs, updates, rolls back, enables, disables, forgets, restores and uninstalls `d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** the lines are `installed d135a7cf-1c63-549a-93cb-8f89656b2ced@V from SOURCE` (or `d135a7cf-1c63-549a-93cb-8f89656b2ced already installed (V)`, optionally with `; run update`), `d135a7cf-1c63-549a-93cb-8f89656b2ced updated A → B` (or `d135a7cf-1c63-549a-93cb-8f89656b2ced up to date (V)`), `d135a7cf-1c63-549a-93cb-8f89656b2ced rolled back B → A`, `d135a7cf-1c63-549a-93cb-8f89656b2ced enabled`, `d135a7cf-1c63-549a-93cb-8f89656b2ced disabled`, `d135a7cf-1c63-549a-93cb-8f89656b2ced forgotten (clone kept; restore re-registers it)` (or, when `ForgetResult.clone_kept` is false, `d135a7cf-1c63-549a-93cb-8f89656b2ced forgotten (no clone; record and dist removed)`), `d135a7cf-1c63-549a-93cb-8f89656b2ced restored V` and `d135a7cf-1c63-549a-93cb-8f89656b2ced uninstalled`
- **AND** `download` prints `downloaded d135a7cf-1c63-549a-93cb-8f89656b2ced@V from SOURCE (disabled)`; install and download print one such line per package, including each dependency pulled from the catalog
- **AND** an `update` or `update all` line of a change marked `major` ends with ` (major)` (`d135a7cf-1c63-549a-93cb-8f89656b2ced updated 1.2.3 → 2.0.0 (major)`; updates, "Major updates are marked"), covered by `update_lines_mark_major_updates`

#### Scenario: Fork, replace and commit verbs
- **WHEN** the user replaces a package or returns through replace ORIGINAL --with ORIGINAL
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring

### Requirement: Errors and exit codes
On failure the CLI SHALL print `Error: MESSAGE` to stderr, for a library error also `Error kind: KIND` (and one `  - item` line per lost item of `local_changes` outside update and rollback, or `has_dependents`) and, where a gitpm command helps, one `Hint: …` line naming it; usage errors print no kind. It SHALL exit with status 1 on failure, 0 on success. Verified by `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`, `update_all_local_work_prints_the_library_message_unchanged`.

#### Scenario: Unknown ID
- **WHEN** the user runs `gitpm update 918a06b8-4971-5358-8ae7-82d0218459ec`
- **THEN** stderr has `Error: package 918a06b8-4971-5358-8ae7-82d0218459ec is not installed` and `Error kind: not_installed`, and the exit status is 1

#### Scenario: Lost work in the update message
- **WHEN** `gitpm update all` is refused because the clone of `d135a7cf-1c63-549a-93cb-8f89656b2ced` holds an unpublished tag `local-only`
- **THEN** stderr has `Error: d135a7cf-1c63-549a-93cb-8f89656b2ced: unpublished local work blocks update all; unpublished tag: local-only` and `Error kind: local_changes`, with no `  - item` line

#### Scenario: Command hint
- **WHEN** the user installs `d135a7cf-1c63-549a-93cb-8f89656b2ced` again after forgetting it while its clone is retained
- **THEN** stderr has the neutral library message, `Error kind: already_installed` and a `Hint:` line naming `gitpm restore d135a7cf-1c63-549a-93cb-8f89656b2ced` and `gitpm uninstall d135a7cf-1c63-549a-93cb-8f89656b2ced`

#### Scenario: One refusal tells everything
- **WHEN** `gitpm uninstall lib` is refused because `app` depends on `lib` and the clone also has a modified file
- **THEN** stderr names `app`, lists the modified file as `  - …` and prints `Error kind: has_dependents`, from the one library error
