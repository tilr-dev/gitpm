# Proposal: Moved and deleted release tags are not gitpm's problem

## Why

The owner decided on 2026-10-18 (decision D5, TILR-39) that gitpm does not police release tags: moved or retagged releases must not cause `version_conflict` anywhere, a retracted installed release is "up to date" with a warning, and integrity of published versions comes only from a verified catalog (`protected-catalog`). Today a moved tag fails single updates, `update all`, updates of siblings in a shared clone and `catalog-update` (spec `package-ids` "Moved release tags are refused"), and a deleted installed tag makes `update ID` fail with a misleading "downgrading requires an explicit ref" while `update all` reports it up to date (review finding G2#2). Rollback to a commit that was pruned after its tag disappeared has no defined error. Rollback checks local work offline and treats the upstream tags in a fork's clone as `UnpublishedTag`, so it refuses a rollback that update would allow, because update classifies tags the origin has as published (spec `updates` "Local work blocks update and rollback", scenario "Retained upstream tag on rollback"; TILR-25).

## What Changes

- **BREAKING** No `version_conflict` for moved tags: fetches take the origin's tags as they are in own and shared clones; installed revisions are kept by commit.
- Release-mode updates compare versions only: a moved tag of the installed version → up to date + warning `release tag ID@V now points to COMMIT; the installed commit is kept`; a retracted installed release → up to date + warning `installed release ID@V no longer exists on the origin`, for `update ID` and `update all` alike.
- `catalog-update` never replaces a published record: a moved tag, or a manifest or file hashes that differ from the record, keep the record and warn instead of `version_conflict` (owner decision of 2026-10-18 on TILR-34). `register` of a conflicting version still fails with `version_conflict`. A verified catalog refuses `catalog-update` altogether; that rule comes with `protected-catalog`.
- Rollback to a previous commit that is no longer in the clone fails with `no_previous_version` naming ID and commit.
- Rollback classifies tags like update: a tag the origin had with the same object at the last fetch is published; rollback stays offline (TILR-25).
- FFI: the tag warnings reach `RevisionChange.warnings` and `UpdateAllPlan.unchanged[].warnings`.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`, `removal-and-restore-rules`. `protected-catalog` builds on the "integrity via protected catalog" requirement added here.

## Capabilities

### Modified Capabilities
- `package-ids`: new "Tags are not policed; integrity via protected catalog"; removed "Moved release tags are refused".
- `updates`: "Stable update never downgrades or accepts mutated releases" (renamed), "Update mirrors the origin's tags", "update all honors each package's mode and refuses local work", "Rollback swaps current and previous", "Local work blocks update and rollback" (TILR-25).
- `ffi-bindings`: new "Tag warnings in records".
- `multi-package-repos`: "One shared clone per source".
- `catalog`: "Published versions are immutable".

## Out of scope

- Hash verification of updates (`protected-catalog`).
- Branch-mode history rewrites (still refused as diverged, `single-ref-mode`).

## Impact

`workspace/clones.rs` (no moved-tag checks, one fetch helper), `workspace.rs` update selection, warnings and rollback error, `lost_work.rs` (origin tags of the last fetch under `refs/origin-tags/`), `catalog.rs` regenerate path, `error.rs` (`NoPreviousVersion.commit`), FFI records, README "Changed in 0.6", tests that asserted `version_conflict` on retagging (renamed and inverted), Dagger moved-tag cases and a fork → update → rollback case. Tracker: TILR-39, TILR-25.

Implemented in commit 661dd01 (feat!: moved release tags never block updates; rollback sees origin tags).
