# Tasks

## 1. Library

- [ ] 1.1 Blocked namespace paths put their active packages in `skipped` (TILR-51); verify `a_namespace_path_that_is_not_a_folder_blocks_nothing_else` extended with `sync-dist` failing `not_deployed`
- [ ] 1.2 New `state.json` via temp + rename; `.DS_Store`-only root is empty (TILR-52); verify `workspace::open_creates_the_workspace_layout_and_lock` and a `.DS_Store` test
- [ ] 1.3 `exec.rs`: empty `GIT_ASKPASS`, `-c credential.interactive=never`, one SSH command for clones and `auth check`, 10-minute kill for `auth check`, `Options.custom_ssh` (TILR-53); verify `exec::git_never_reads_stdin_or_prompts` (environment and args asserted) and `auth::failure_maps_every_category_and_hides_stderr`
- [ ] 1.4 fsync journal, `state.json` and parent folders at commit (TILR-54); verify `txn::commit_syncs_journal_state_and_parents` (sync calls through a test hook)
- [ ] 1.5 `fork --replace` release step like `commit` (TILR-57); verify a hook-rejected release restores manifests/index with no tags, and a wrong-typed metadata key fails with `invalid_manifest`
- [ ] 1.6 Identity via `git -c user.useConfigOnly=true var`; `invalid_argument` (TILR-49); verify `scaffold::derived_identity_is_refused` and `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`
- [ ] 1.7 Deploy and hash the committed tree without applying Git attributes (TILR-46); verify a test with `export-ignore` and `export-subst` in `.gitattributes` (deployed bytes equal hashed bytes)
- [ ] 1.8 Lock: check the process-wide set by canonical path before opening the lock file; never open and close a second descriptor of a held lock file (or switch to `flock`/OFD locks), for the workspace and the catalog lock (TILR-45); verify `lock::refused_second_open_keeps_the_first_lock` (a refused in-process open, then a child process still gets `locked`) and the same for two catalog writers
- [ ] 1.9 Messages name the package: "COMMIT changed without a version bump", "history of BRANCH diverged", "rollback manifest mismatch", "package id changed upstream" and the `update all` local-work refusal carry the ID; FFI `PmError` gets `id` for them (TILR-50); verify `error::package_errors_name_the_package` and `ids_and_lost_work_items_are_preserved`
- [ ] 1.10 `auth check` ls-remote and the `core.sshCommand` probe get the 10-minute kill and the batch SSH options (TILR-24); verify `exec::git_never_reads_stdin_or_prompts` (probe args/timeout asserted) and `auth::access_check_is_bounded` against a silent server
- [ ] 1.11 Refuse submodules (gitlinks) inside a package dist with `unsupported` before deploying (TILR-26); verify `a_submodule_in_dist_is_refused` (install fails, workspace unchanged)

## 2. CLI

- [ ] 2.1 Derive `custom_ssh` from `GIT_SSH_COMMAND`/`GIT_SSH` for every command; verify `gitpm_variables_set_root_catalog_and_sources` extended

## 3. FFI

- [ ] 3.1 `ManagerOptions.customSsh`; verify `records_convert_field_by_field` and `manager_round_trip_on_empty_workspace`

## 4. Specs, tests, docs

- [ ] 4.1 Dagger: blocked namespace with `sync-dist`, `.DS_Store` root, `GIT_SSH` install, export-ignore deploy, submodule in dist refused, `auth check` against a silent server; verify `./itests/dt --only scenarios`
- [ ] 4.2 README rules (SSH policy, crash definition, identity) and "Changed in 0.6"; verify by reading against the specs
- [ ] 4.3 `openspec/config.yaml` context: "the CLI also derives `custom_ssh` for `auth check`" becomes "the CLI derives `Options.custom_ssh` from GIT_SSH_COMMAND/GIT_SSH for every command", and the Subprocesses paragraph drops the `auth check` and `core.sshCommand`-probe exceptions (one SSH policy, every git call bounded); verify by reading it against spec `git-safety`
- [ ] 4.4 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
