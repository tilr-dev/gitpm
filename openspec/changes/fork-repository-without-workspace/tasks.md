# Tasks

## 0. Decision

- [ ] 0.1 Owner confirms the feature is wanted and the CLI form (`fork SOURCE [ID...]` vs a separate `fork-repo` command); record the answer in proposal.md

## 1. Library

- [ ] 1.1 Extract the source-independent part of `fork_packages` (tree copy, sibling removal, gitpm.json rewrite, dependency rewrite) so it can start from a fresh clone at a resolved revision; verify existing fork tests still pass (`cargo test -p gitpm fork`)
- [ ] 1.2 Add a free function forking a source at the latest release or `--ref` into `DIR` without a workspace; unit test on a two-package repository asserts only the chosen package is left and `forkOf.commit` is the resolved commit

## 2. CLI and docs

- [ ] 2.1 Parse `fork SOURCE [ID...] --ns NS --into DIR [--ref REF]`; CLI test asserts no workspace folder is created and the output lines match the installed-package fork
- [ ] 2.2 Document the form in README (fork section) and crates/gitpm-cli/README.md; verify the README example runs as written
