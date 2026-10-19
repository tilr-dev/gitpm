## MODIFIED Requirements

### Requirement: Settings come from flags, environment, defaults
The CLI SHALL resolve `--root DIR` / `GITPM_ROOT` (default `./gitpm-workspace`), `--catalog LOC` / `GITPM_CATALOG` (no default), `--sources FILE` / `GITPM_SOURCES` (default `./sources.txt`), flag first. There SHALL be no config file; `PM_*` variables SHALL be ignored. Verified by `custom_ssh_is_shared_by_auth_install_catalog_and_check`, `gitpm_variables_set_root_catalog_and_sources`, `legacy_pm_variables_are_ignored`, `defaults_are_relative_to_the_current_directory`.

#### Scenario: SSH variables for every command
- **WHEN** `GIT_SSH_COMMAND` or `GIT_SSH` is set and the user runs any command, `auth check` included
- **THEN** the CLI sets `Options.custom_ssh` for the library (git-safety, "One subprocess boundary")

#### Scenario: Legacy variable
- **WHEN** `PM_ROOT=/elsewhere` is set and `GITPM_ROOT` is not
- **THEN** the workspace is `./gitpm-workspace`
