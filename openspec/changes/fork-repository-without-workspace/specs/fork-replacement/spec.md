## ADDED Requirements

### Requirement: Fork a repository source without a workspace
`fork SOURCE [ID...] --ns NAMESPACE --into DIR`, where `SOURCE` is a Git path or URL, SHALL fork the named packages of that repository (its only package when it has one) at its latest release or `--ref` into `DIR` under `NAMESPACE/<same name>`, with the same destination, sibling-removal, dependency-rewrite and `forkOf`/`replaces` rules as forking installed packages, and SHALL NOT open or create a workspace.

#### Scenario: Fork a repository directly
- **WHEN** the user runs `gitpm fork ./tilr-plugins tilr/theme --ns me --into me-theme` with no workspace
- **THEN** `me-theme` contains only `packages/theme` with `id` `me/theme` and `forkOf.commit` equal to the `tilr/theme` release commit, and no `gitpm-workspace` folder exists

#### Scenario: Ambiguous repository
- **WHEN** the source has several packages and no ID is given
- **THEN** the command fails listing the packages, as `install` does
