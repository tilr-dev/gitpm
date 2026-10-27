# Proposal: Hardening: validation, locks, journaled transactions, git safety

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Phase 1 of the repair plan in Basic Memory note "Tilr PM — use cases и план починки (todo)" listed library defects found in review: false `locked` on reopen (flock inherited by children), a symlink outside `dist/` bricking the workspace, one broken historical tag breaking discovery, stale local branches for `update --ref`, unvalidated manifest versions, empty `files` skipping hash checks, catalog locks surviving killed writers, wrong error kind for restore, IDs starting with `-` reaching git, case collisions on APFS, git clone/fetch able to hang forever holding the lock, plain-HTTP catalogs open to MITM, and author `commit` leaving a modified manifest after a failed commit. The owner decided that the dev channel's identity is the commit.

## What Changes

- Manifest and `forkOf` versions must be MAJOR.MINOR.PATCH.
- Broken historical release tags are skipped and reported; the newest tag must still be valid.
- `update --ref` resolves tag → `refs/remotes/origin/X` → commit; local `refs/heads/*` refused.
- Dev channel: a new upstream commit updates without a version bump.
- Namespaced catalog releases require file hashes.
- `restore` of a missing repository reports `repository_not_found`.
- ID components may not start with `-`; git options end before names.
- IDs differing from installed ones only in letter case are refused.
- Every git subprocess is noninteractive and bounded (closed stdin, no prompts, low-speed limits, batch SSH, 10-minute kill).
- Catalog over HTTPS only (HTTP only for loopback); curl limited to http(s), 16 MiB, no redirects.
- Workspace lock: process-owned fcntl record lock keyed by the real path, plus a process-wide set.
- Catalog writer lock beside the catalog, unique temp files, leftovers cleaned.
- Journaled transactions staging only touched paths in `<root>/.pm-txn`, `state.json` replaced last; recovery rolls forward or discards.
- Author `commit`: a failed git commit restores `plugin.json` and the index; a tag-only failure can be retried.

## Capabilities

### Modified Capabilities
- `package-ids`
- `updates`
- `catalog`
- `git-safety`
- `transactions-locking`
- `authoring`

## Out of scope

- Streaming enforcement of the 16 MiB limit when no Content-Length is sent (found later, postponed).
- Embedded git/HTTP (gix, ureq).

## Impact

Implemented by: [`9734d9f`](https://github.com/tilr-dev/gitpm/commit/9734d9f), [`aa1b07f`](https://github.com/tilr-dev/gitpm/commit/aa1b07f), [`ef3fb92`](https://github.com/tilr-dev/gitpm/commit/ef3fb92), [`05010e7`](https://github.com/tilr-dev/gitpm/commit/05010e7), [`cc381dc`](https://github.com/tilr-dev/gitpm/commit/cc381dc), [`5e58e3d`](https://github.com/tilr-dev/gitpm/commit/5e58e3d), [`b597bc8`](https://github.com/tilr-dev/gitpm/commit/b597bc8), [`aaf736a`](https://github.com/tilr-dev/gitpm/commit/aaf736a), [`cadabec`](https://github.com/tilr-dev/gitpm/commit/cadabec), [`a8eceaa`](https://github.com/tilr-dev/gitpm/commit/a8eceaa), [`91e13b8`](https://github.com/tilr-dev/gitpm/commit/91e13b8), [`f8824f2`](https://github.com/tilr-dev/gitpm/commit/f8824f2), [`10e3cc8`](https://github.com/tilr-dev/gitpm/commit/10e3cc8), [`86231c9`](https://github.com/tilr-dev/gitpm/commit/86231c9) (all 2026-10-15 21:20–21:38).
