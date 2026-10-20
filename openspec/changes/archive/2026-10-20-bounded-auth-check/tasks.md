# Tasks

## 1. Library

- [x] 1.1 `auth check` ls-remote and the `core.sshCommand` probe get a 10-second kill, and the `auth check` SSH command gets keepalives (TILR-24); verify `exec::git_never_reads_stdin_or_prompts` (probe args/timeout asserted) and `auth::access_check_is_bounded` against a silent server (fails with a timeout in about 10 seconds)

## 2. Specs, tests, docs

- [x] 2.1 Dagger: `auth check` against a silent server; verify `./itests/dt --only scenarios`
- [x] 2.2 `openspec/config.yaml` context: the Subprocesses paragraph drops the "no wall-clock kill" exception of the `auth check` ls-remote and the "no timeout" of the `core.sshCommand` probe (both bounded by 10 seconds); verify by reading it against spec `git-safety`
- [x] 2.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
