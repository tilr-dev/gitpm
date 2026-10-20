# Proposal: fork --replace releases like commit

## Why

The release step of `fork --replace` reads metadata leniently, does not restore the index when the release commit fails and may leave some tags behind, unlike `commit` (review 2, G2#10, TILR-57).

## What Changes

- The release step of `fork --replace` uses the release path of `commit`: strict metadata (`invalid_manifest`), manifests and index restored on failure, tags all or none.

Depends on: the 2026-10-18 chain through `host-compatibility` (`no-backward-compat` … `host-compatibility`); the deltas are written against the specs as that chain leaves them.

## Capabilities

### Modified Capabilities
- `fork-replacement`: "Fork and replace in one step".
- `plugin-metadata`: "Lenient reading, strict authoring".

## Impact

`workspace/fork.rs` (release step), shared release code with `commit`. Tracker: TILR-57. Split from the former shared change `robustness-fixes` (2026-10-20: one task, one change).

## Implementation

Implemented in commit `47489c9` (Task: TILR-57).

The fork release previously skipped strict metadata validation, restored only
manifests after staging failed commits, and created tags one by one. The shared
authoring path validates all manifests before writes, restores manifests and the
index on commit failure, and creates batch tags atomically with a local fetch.
Hooks and a locked second tag reproduce the failures deterministically; all three
new integration tests fail on the original code.
