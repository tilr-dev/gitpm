# Proposal: Fork packages of a repository that is not installed (`fork ID… --from SOURCE`)

## Why

`fork` only accepts IDs installed in a workspace: it copies the installed clone (`Manager::fork_packages` looks the IDs up in `state.json`). While moving the test bench to a multi-package repository the owner asked whether a repository could be forked directly, without installing it first (Basic Memory note "gitpm — крейт на crates.io…", section "Бэклог: fork без workspace", 2026-10-17; bench TODO.txt). The owner decided on 2026-10-18 (decision D13, TILR-14): yes, as `fork ID… --from SOURCE`, implemented in the library as a free function and exposed through the FFI. Status: verified not implemented on `dev` (0b146cf).

## What Changes

- New form `gitpm fork ID [ID...] --from SOURCE --ns NAMESPACE --into DIR [--ref REF]`: `SOURCE` is a Git path or URL, the IDs name packages of that repository (none needed when it has one package), forked at its latest release or at `--ref`. The `--from` marker means the CLI never guesses whether a positional is a path or an ID (thin CLI, D2).
- Same destination, sibling-removal, dependency-rewrite and `forkOf`/`replaces` rules as forking installed packages; `forkOf.commit` is the resolved commit; no workspace is opened, created or locked.
- Library: a free function (no `Manager`, no workspace lock), e.g. `fork::fork_source(source, ids, reference, namespace, destination)`, sharing the tree-copy, sibling-removal, `gitpm.json` and dependency-rewrite code with `Manager::fork_packages`.
- FFI: the same free function (`forkSource`), for CLI/FFI parity.
- `--replace` is refused with `--from` (it needs a workspace; install and replace afterwards).

## Capabilities

### Modified Capabilities
- `fork-replacement`: "Fork copies an installed package" (installed IDs, or IDs of `--from SOURCE`).
- `multi-package-repos`: "Forking packages of one repository" (the same repository: the installed source or `--from`).
- `cli`: "Option validation" (`--from` and `--ref` are valid for `fork`; `--ref` without `--from` and `--replace` with `--from` are refused).

## Out of scope

- `--replace` together with `--from`.
- Forking local, uncommitted edits (there is no installed clone).
- Adding `--from` packages to an existing fork repository beyond what the installed-package form already allows (same rules apply).

## Impact

`crates/gitpm/src/workspace/fork.rs` (shared fork logic, new free function), `crates/gitpm-ffi/src/lib.rs` (`forkSource`), `crates/gitpm-cli/src/main.rs` (option parsing), README fork section and `crates/gitpm-cli/README.md`. Tracker: TILR-14. The `cli` delta touches "Option validation", which `cli-catalog-commands` also modifies; whichever is archived second rebases its block on the first.
