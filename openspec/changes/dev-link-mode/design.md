# Live dev links: research and implementation design

## Status and evidence

Research dated 2026-10-20; implementation is separate.
Owner decisions are comments #833–836 (2026-10-19) and the unified publishing-folder decision (2026-10-20).
Their text is copied into the issue's `Решения` section. Recommendations below
are technical proposals, not additional owner decisions.

Baseline inspected: gitpm dev commit `e31c1ac` (2026-10-22). Recheck with `git rev-parse HEAD`
and the paths below when implementing; these observations are not environment
limitations or workarounds:

- `openspec/specs/install-uninstall/spec.md`, "Install an explicit ref or a branch",
  and `crates/gitpm/src/model.rs:159`: `--dev` is origin-default-branch following;
  state Mode has Release, Pinned and Branch. The old channel problem is resolved.
- `openspec/specs/dist/spec.md`, "Deploys copy the committed tree": managed
  deployments select the committed manifest `dist` folder (currently defaulting to
  `<path>/dist`) from Git objects, not live files. A link is an
  explicit exception, not an alternate implementation of that copy operation.
- `openspec/specs/workspace-layout/spec.md`, "Installed package record", and
  `crates/gitpm/src/model.rs:133`: current/previous revisions assume an exact
  commit. Uncommitted bytes cannot truthfully be described by that revision.
- `openspec/specs/transactions-locking/spec.md`: staged paths, journal, state-last
  commit, roll-forward recovery, real managed parents. A leaf symlink must not
  relax the managed-parent checks.
- `crates/gitpm/src/check.rs:86`, `crates/gitpm/src/workspace.rs:113` and
  `crates/gitpm-ffi/src/lib.rs:303`: mode and update/check reasons already cross
  public boundaries, but none represents a live link. Adding a CLI-only warning
  would not satisfy comment #836.

Prior owner decisions retained: one fixed workspace layout (Basic Memory decision
2026-10-18, "gitpm — настраиваемые Layout, Options и TagScheme не делаем") and
packages as folders in multi-package repositories (decision 2026-10-16,
"gitpm — несколько пакетов-папок в одном репо через gitpm.json"). Link keeps
`dist/<namespace>/<name>` and resolves a selected package folder, not necessarily
the repository root.

## Alternatives (original question 1)

| Variant | Data exposed to the host | Refresh | Assessment |
| --- | --- | --- | --- |
| (a) Follow origin HEAD | Committed manifest-selected folder at the fetched branch commit | Explicit update | Already available through `--dev` / branch mode; cannot expose uncommitted edits |
| (b) Link author working tree | Author's installation folder through a symlink | File edits become visible through the path immediately | Selected by Dan; author handles build/watch |
| (c) Both under one dev/channel switch | Either of the above depending on source | Ambiguous | Keep both capabilities with explicit distinct operations, not an overloaded switch |

For (b), copying the dirty working tree is a snapshot, needs repeated sync and
cannot provide immediate visibility. A symlink provides that visibility with no
gitpm daemon. Hard links do not represent directories and would share mutable
files in surprising ways; a workspace-owned copy adds ownership/lost-work risk.
The owner selected the symlink. A host may still need to reload a package: gitpm
only makes the path resolve to the latest author-written bytes.

## Entry point and folder resolution

Recommend a distinct `link LOCAL_PACKAGE_PATH` command and matching Manager/FFI
method, rather than `install --dev`: the latter already names remote branch
following. The library validates and resolves the local package and returns all
domain errors; CLI only parses paths and prints results. No remote URL is a link
target. A selected package folder inside a multi-package repository is supported.

Dan selected a unified publishing-folder model for managed install and link:
use `plugin.json.dist` when configured, otherwise the package root (the repository
root for a root package). For a multi-package repository, resolve the configured
path relative to the selected package folder. The existing field accepts `"."`
for the root and validates package-relative paths; reuse that schema and its
`buildCommand` / `buildRequired` metadata. gitpm never runs builds or watchers.

The inspected baseline defaults `dist` to `"dist"`, including the requirements
"Package folder contents" and "Known manifest keys" in package-manifest.
The selected root default is a future behavior change for both managed install
and link, not a description of current behavior. Its implementation must align
those requirements and the missing-output refusal with the dist deployment
contract. Existing packages needing `dist/` must declare `dist: "dist"` explicitly.

Resolve an existing directory to a canonical absolute target; reject paths that
would create a workspace/self-reference cycle. Validate identity and manifest (`invalid_manifest`)
before staging anything. Record an absolute path so CLI cwd changes cannot retarget
a link. A missing configured output is an error, not an instruction to run a build.
No build script or hook from the package is executed. Existing source validation
rules still govern any optional origin used for release discovery.

For already-recorded IDs, recommend explicit conflict handling rather than
silently replacing another source. Exact same-link reentry can be idempotent.
Converting a managed installation to/from link must be an explicit library
operation with a plan, including lost-work checks for any managed clone it removes.
No existing `update` target should implicitly detach a link. These conversion API
details are recommendations for implementation review, not new owner decisions.

## State and revisions (original question 3)

Dan requires an explicit link marker and delegates the concrete format to this
design. Recommend a discriminated package record: managed packages retain their
revision/mode history; link records carry `mode` as `{"link": {...}}` — the same object
shape as the current `{"pinned": REF}` / `{"branch": NAME}` — with canonical
package and installation paths. Do not fake a clone key,
commit or `previous` revision for a linked folder.

Link record format (decided here, as Dan delegated):

```json
{
  "mode": {
    "link": {
      "packageRoot": "/work/author/packages/example",
      "target": "/work/author/packages/example/build"
    }
  },
  "enabled": true,
  "version": "0.3.0",
  "source": "/work/author",
  "dependencies": {},
  "metadata": {}
}
```

The snippet is one package record, not a complete state document. Required:
`mode.link.packageRoot`, `mode.link.target`, `enabled`, `version` (declared),
`dependencies`, `metadata`; `source` when available. Absent: `current`,
`previous`, `clone`, `path`, `catalog`, `forkOf`, `replaces`. The manifest identity/version/dependencies/metadata are a
validated snapshot at link time; file content stays live. They do not silently
change the dependency graph when the author edits the manifest. An explicit
relink refreshes that snapshot after validation and dependency checks. A displayed
version is a declared version, not a reproducibility guarantee. Source identifies
the release-discovery repository; it is not proof of workspace ownership. When
there is no usable origin, return an availability problem instead of inventing a
latest release. A local repository path may be a valid source under existing rules.

This is **BREAKING**: change the state format from the version current at
implementation time, refuse foreign formats unchanged, document workspace
recreation. On the inspected baseline it is format 3; do not hard-code the next
number now. No migration reader. Forgotten link records retain the link descriptor,
not an owned-clone assumption. Rollback does not roll back external files:
rollback of a link fails with `no_previous_version` and changes nothing, until an
explicit conversion history contract exists (install-uninstall delta).

## Deployment, transactions and lost work (original question 2)

For active links, `dist/<id>` is the symlink itself. Disabled, replaced, forgotten
and uninstalled packages have no deployed entry, as for managed installations.
Enable/restore/reconcile recreate the entry using the descriptor; they do not
call committed-tree deployment or require a workspace clone. An unavailable
target is reported structurally as not deployed, with its link mode preserved.
Unrelated packages remain operable under existing reconciliation rules.

Stage the symlink entry under `.pm-txn`, then journal entry renames/removals as
with other touched paths, state last. On failure before the journal, discard only
staged entries. After journal commit, recovery rolls entry operations forward
idempotently. Use non-following metadata and unlink/rename semantics: dangling
links are entries too. Recovery and transaction cleanup must never recurse into a
link target, even if the author changes/removes it after planning. Continue to
refuse symlinks at `dist`, `dist/<namespace>`, `packages` or other managed parents.
An unexpected leaf target is a conflict, not permission to traverse it.

Atomicity covers the workspace entries and state only. External contents are not
locked, snapshotted, rolled back or promised to remain stable for readers.
The author can edit while gitpm commits; those edits are intentional, not lost
work. Build/watch output follows the same rule. The host cannot expect a whole
directory of build output to change atomically through a symlink.

Removal plans list the owned symlink/state changes; the external folder is never
in `deletes`. Dirty/staged/untracked/ignored files, unpublished refs and stashes
in the external repository do not block removal of its link, because none is
deleted. Force and confirmed-lost-work options cannot authorize target deletion.
Managed clones keep all existing lost-work protections, including when switching
away from a managed installation. Linking a workspace-owned checkout must not
permit its later removal as if no external user relied on it: recommend refusing
targets within this workspace's managed storage, with a clear `workspace` error.

## Release checks and structured reporting (original questions 3–4)

Linked packages are immutable candidates in the joint dependency solve: hold the
recorded version/dependencies as a fixed constraint, never checkout/fetch into
their author tree or install replacement dependencies on their behalf. Healthy
managed packages can still update. Link mode remains visible even if another
dependency or origin problem is also reported; it must not disappear under reason
precedence. Existing release/pre-release and catalog-protection policies apply to
availability discovery, not to changing linked files.

List/state expose the link mode, target and recorded manifest data. Check and both
update forms expose `dev_linked`, unchanged status and optional available release
version. Keep link disposition separate from discovery failure/availability data,
so unreachable origin is not reported as up to date. Read-only check must not
create workspace folders, clones or build output. `update all` dry-run reports
the same link disposition without writes. New-release discovery uses the existing
non-mutating release-discovery path; do not fetch into the author's repository.
For a fork, keep the existing distinction between the package's origin releases
and its original repository's upstream information.

Example CLI rendering of library data:
`acme/example: version 0.4.0 is available; package is dev-linked and was not updated`.
Without a newer release, still report dev-linked and unchanged; discovery failures
are explicit. The CLI must not compare versions or synthesize domain reasons.

Channel/Optional-Channel is obsolete: FFI already has Target and recorded Mode.
Extend the relevant public record/enum representations for link and availability,
including readState, list, check, single update, updateAll and removal plans.
An availability field is optional because discovery may fail or find no release;
the recorded link marker is mandatory for a link. Match library data in FFI,
including error kinds. A public link method is required alongside the CLI.

## Implementation and verification plan

See `tasks.md` for future work, deliberately unchecked until implemented. Use
hermetic local repositories and fixture directories, not public services.

- Edit a target file after linking and read the deployed path immediately; assert
  changed bytes with no gitpm call, commit, timer or watcher.
- Assert `read_link` and state/FFI fields, root and multi-package path selection,
  configured output, missing outputs and invalid identity refusals.
- Seed dirty files, stash and unpublished refs in the external tree; remove/force
  remove/forget/disable/reconcile and compare its bytes/refs before and after.
- Use explicit transaction failpoints at journal/rename/state boundaries, restart
  and verify convergence and preservation of external edits. No sleep or random
  process timing. Cover dangling targets and malicious managed-parent symlinks.
- Create a newer release tag in a local source and verify structured library/FFI
  results for check/update/update-all/dry-run, CLI rendering and unchanged link.
  Include unreachable source, no newer release and dependency-constrained updates.
- Keep managed install/update/deploy/lost-work regression coverage; document the
  new format and command alongside the implementation.

## Handoff

This research opens the change without archiving it (Dan, 2026-10-20).
A separate implementation task owns the remaining work and archive.
