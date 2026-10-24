## MODIFIED Requirements

### Requirement: Option validation
Before calling the library the CLI SHALL refuse only syntax: unknown or repeated options, misplaced options (for example `--dry-run` outside `update all`/`sync-dist`), exclusive options (`--ref`, `--dev`, `--release`), values outside a fixed set and wrong argument counts, with a message ending `; use --help` where applicable (usage errors without `Error kind`). Domain combinations are checked by the library. Verified by `usage_errors_name_the_problem_and_create_nothing`.

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

### Requirement: One output line per affected package
Every workspace-mutating command (`install`, `install-catalog`, `download`, `download-catalog`, `update`, `update all`, `rollback`, `enable`, `disable`, `forget`, `restore`, `uninstall`, `replace`, `sync-dist`) SHALL print one line per affected package in the form `ID <verb> …` (scenario "Single-package verbs"), built from the library result alone without reading state, and SHALL NOT print `OK:` lines. Other commands print what their capability specifies.

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
- **WHEN** the user replaces, switches back
- **THEN** the lines are `ORIGINAL replaced by FORK V [(as ORIGINAL V')] for: consumers` and `ORIGINAL switched back from FORK (…)` (exact variants in fork-replacement), built from the `ReplacementPlan`; the `fork` and `commit` lines are specified in fork-replacement and authoring
