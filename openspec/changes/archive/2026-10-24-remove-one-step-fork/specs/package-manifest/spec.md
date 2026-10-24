## MODIFIED Requirements

### Requirement: Unknown keys are preserved
Single source for unknown keys: top-level keys gitpm does not know SHALL be accepted and SHALL be written back unchanged whenever gitpm rewrites the manifest (`commit`, `fork`), after the known keys and in their original order relative to each other. The metadata keys (plugin-metadata) count as unknown keys for this rule. Verified by `model::manifest_keeps_unknown_top_level_keys_through_read_and_write` and `model::manifest_keeps_the_original_order_of_unknown_keys`.

#### Scenario: Host metadata survives a release
- **WHEN** the manifest contains a host-specific key `"main": "index.js"` and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the rewritten `plugin.json` has `version` `0.2.0` and still contains `"main": "index.js"`, written after the known keys
