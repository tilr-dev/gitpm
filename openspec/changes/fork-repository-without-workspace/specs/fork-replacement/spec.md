## MODIFIED Requirements

### Requirement: Fork copies an installed package
`fork ID [ID...] --ns NAMESPACE --into DIR` SHALL copy installed packages (with Git history and local edits of the installed clone) into the fork repository `DIR` under the IDs `NAMESPACE/<same name>`; the original installation SHALL be unchanged and the original repository SHALL become the `upstream` remote. Without `--from` only installed IDs are accepted; with `--from SOURCE` the IDs name packages of that repository (see the scenarios). Verified by `fork_preserves_history_edits_and_original_installation`.

#### Scenario: Fork one package
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into hello-fork`
- **THEN** `hello-fork` is a Git repository whose `plugin.json` has `id` `me/hello`, the output contains `me/hello forked from acme/hello (repository root)`, and `acme/hello` is still installed

#### Scenario: Fork a repository that is not installed
- **WHEN** no workspace exists and the user runs `gitpm fork tilr/theme --from ./tilr-plugins --ns me --into me-theme`
- **THEN** `me-theme` contains only `packages/theme` with `id` `me/theme` and `forkOf.commit` equal to the `tilr/theme` release commit, and no workspace folder is created

#### Scenario: Ambiguous source
- **WHEN** the source of `--from` has several packages and no ID is given
- **THEN** the command fails with `invalid_argument` listing the packages, as `install` does
