## MODIFIED Requirements

### Requirement: Settings come from flags, environment, defaults
The CLI SHALL resolve `--root DIR` / `GITPM_ROOT` (default `./gitpm-workspace`), `--catalog LOC` / `GITPM_CATALOG` (no default), `--sources FILE` / `GITPM_SOURCES` (default `./sources.txt`) and `--host NAME@VERSION` / `GITPM_HOST` (no default; `Options.host`), flag first. There SHALL be no config file; `PM_*` variables SHALL be ignored. Verified by `gitpm_variables_set_root_catalog_and_sources`, `legacy_pm_variables_are_ignored`, `defaults_are_relative_to_the_current_directory`.

#### Scenario: SSH variables for auth check
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs `gitpm auth check URL`
- **THEN** the CLI passes `custom_ssh` to the library (git-safety, "One subprocess boundary")

#### Scenario: Legacy variable
- **WHEN** `PM_ROOT=/elsewhere` is set and `GITPM_ROOT` is not
- **THEN** the workspace is `./gitpm-workspace`

#### Scenario: Host setting
- **WHEN** `GITPM_HOST=tilr@2.5.0` is set and the user runs `gitpm update all`
- **THEN** the library gets `Options.host` `{name: "tilr", version: "2.5.0"}`; a value without `@` or with an invalid version is refused by the library with `invalid_argument`
