# Tasks

## 1. Library

- [x] 1.1 `exec.rs`: one batch SSH command for clones and `auth check`, `Options.custom_ssh` used by auth and every git command (standalone catalog and check `*_with_options`, default wrappers preserved) (TILR-135); verify `exec::git_never_reads_stdin_or_prompts` and `auth::failure_maps_every_category_and_hides_stderr`

## 2. CLI

- [x] 2.1 Derive `custom_ssh` from `GIT_SSH_COMMAND`/`GIT_SSH` for every command; verify `gitpm_variables_set_root_catalog_and_sources` and `custom_ssh_is_shared_by_auth_install_catalog_and_check`

## 3. FFI

- [x] 3.1 `ManagerOptions.customSsh`; verify `records_convert_field_by_field` and `manager_round_trip_on_empty_workspace`

## 4. Specs, tests, docs

- [x] 4.1 Dagger: `GIT_SSH` install; verify `./itests/dt --only scenarios`
- [x] 4.2 README rules (SSH policy) and "Changed in 0.6" (`custom_ssh`, BREAKING); verify by reading against the specs
- [x] 4.3 `openspec/config.yaml` context: "the CLI also derives `custom_ssh` for `auth check`" becomes "the CLI derives `Options.custom_ssh` from GIT_SSH_COMMAND/GIT_SSH for every command", and the Subprocesses paragraph drops the exception that the `auth check` ls-remote uses its own SSH command (one SSH policy); verify by reading it against spec `git-safety`
- [x] 4.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
