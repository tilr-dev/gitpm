# Deploys and hashes use the committed tree

## Why

Git archive applies export attributes while catalog hashes previously used checked-out files, allowing deployed bytes to differ from verified bytes. Gitlinks represent no installed files and should warn rather than block publication or deployment.

## What Changes

- Export committed blobs without applying Git attributes.
- Hash staged exports before transaction commit.
- Materialize gitlinks as empty directories without fetching submodules; warn authors and users that this is probably a mistake and recommend committing built files instead.

## Capabilities

### Modified Capabilities
- `dist`: Deploys copy the committed tree.
- `catalog`: Register one version; Catalog generated from Git release tags.

## Impact

Export, catalog hashing, transaction reports, CLI warnings, README and integration scenarios.
