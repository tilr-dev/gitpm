# Proposal: Fork a repository without a workspace

## Why

`fork` only accepts IDs installed in a workspace: it copies the installed clone (`Manager::fork_packages` looks the IDs up in `state.json`). While moving the test bench to a monorepo, the owner asked whether `fork REPO --ns NS --into DIR` should fork a repository directly, without installing it first; the assistant called it technically simple and the question was left open "awaiting decision" (Basic Memory note "gitpm — крейт на crates.io…", section "Бэклог: fork без workspace", 2026-10-17; bench TODO.txt). Status: verified not implemented on `dev` (0b146cf).

## What Changes

- New form `gitpm fork SOURCE [ID...] --ns NAMESPACE --into DIR` where `SOURCE` is a Git path or URL (never an `a/b` ID, per the one-notation rule), forking the given packages (all packages when the source has one) from a chosen revision (latest release tag, or `--ref`).
- Same destination, sibling-removal, dependency-rewrite and `forkOf`/`replaces` rules as the installed-package fork; no workspace is opened or created.
- Library: a free function (no `Manager`, no workspace lock) mirroring `fork_packages` for a source.

## Capabilities

### Modified Capabilities
- `fork-replacement`: a repository source may be forked without a workspace.

## Out of scope

- `--replace` for this form (it needs a workspace; the existing two-step `install` + `replace` applies).
- Forking local, uncommitted edits (there is no installed clone).
- Deciding whether the feature is wanted at all: this proposal records the option; the owner decides before apply.

## Impact

`crates/gitpm/src/workspace/fork.rs` (shared fork logic), `crates/gitpm-cli/src/main.rs` (argument parsing: a first positional that is a path/URL vs an ID), README fork section, FFI optional.
