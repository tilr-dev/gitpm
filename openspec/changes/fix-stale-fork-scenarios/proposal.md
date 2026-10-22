# Align fork scenarios with recorded-source cloning

## Why

Seven Dagger scenarios still assume that fork copies the installed clone or starts from the source's latest revision. The current fork-replacement contract instead clones the recorded source at installed revisions, excludes local installation work, and refuses existing destinations unchanged.

## What Changes

- Verify that local installation work stays in the installation and is excluded from the fork.
- Exercise branch name collisions in the recorded source.
- Check existing destination refusal and preservation using the current diagnostic.
- Verify repository contents and package revisions at the newest installed revision, including the absence of later uninstalled packages.
- Demonstrate that these assertions detect deliberately broken behavior.

## Capabilities

No new or modified capabilities. Existing fork-replacement requirements remain unchanged; this change updates their Dagger coverage only.

## Impact

Only Dagger scenarios and their supporting assertions change. Fork implementation and other active changes are outside the scope.
