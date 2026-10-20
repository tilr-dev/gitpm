## MODIFIED Requirements

### Requirement: Fork and replace in one step
`fork … --replace [--version V] [--force]` SHALL make a release commit of the forked folders (`version` set by `--version`) and one release tag per package (package-ids), under the rules of `commit` (authoring), then install each fork (dependencies first) and replace its original. If the release fails nothing SHALL be installed; a failed install or replace of one package does not stop the others. Verified by `fork_replace_releases_installs_and_replaces_in_one_step`.

#### Scenario: One-step fork
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into f --replace`
- **THEN** `me/hello@<forked version>` is tagged, installed and replaces `acme/hello`, so only `dist/me/hello/` is deployed

#### Scenario: Failed release commit
- **WHEN** a pre-commit hook rejects the release commit of `fork … --replace`
- **THEN** the forked manifests and the index are as before the release, no tag exists (tags are created all or none), nothing is installed, and the result reports the failed `release` step
- **AND** a wrong-typed metadata key fails the release step with `invalid_manifest` (plugin-metadata)
