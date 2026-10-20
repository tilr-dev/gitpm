## MODIFIED Requirements

### Requirement: Fork and replace in one step
`fork … --replace [--version V] [--force]` SHALL release the forked folders under the rules of `commit` (authoring): one commit (`version` set by `--version`) and one tag per package (`NAMESPACE/NAME@VERSION`, package-ids), then install each fork (dependencies first) and replace its original. If the release fails nothing SHALL be installed; a failed install or replace of one package SHALL NOT stop the others. Verified by `fork_replace_releases_installs_and_replaces_in_one_step`.

#### Scenario: One-step fork
- **WHEN** the user runs `gitpm fork acme/hello --ns me --into f --replace`
- **THEN** `me/hello@<forked version>` is tagged, installed and replaces `acme/hello`, so only `dist/me/hello/` is deployed

#### Scenario: Failed release commit
- **WHEN** a pre-commit hook rejects the release commit of `fork … --replace`
- **THEN** the forked manifests and the index are as before the release, no tag exists (tags are created all or none), nothing is installed, and the result reports the failed `release` step
- **AND** a wrong-typed metadata key fails the release step with `invalid_manifest` (plugin-metadata)
- **AND** verified by `fork_replace_release_rejecting_hook_restores_manifests_and_index` and `fork_replace_release_rejects_wrong_metadata_type`

#### Scenario: Release without file transport
- **WHEN** a multi-package fork is released while Git's file transport is disabled
- **THEN** all release tags are created at the release commit without using a transport
- **AND** verified by `fork_replace_release_without_file_transport`
