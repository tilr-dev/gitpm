# Live dev links: research and implementation design

## Status and evidence

Research dated 2026-10-20 established the contract implemented here.
Owner decisions of 2026-10-19 select live links and structured reporting.
The publishing-folder decision of 2026-10-23 retains the existing `"dist"` default
and supersedes the package-root default selected on 2026-10-20. Technical choices below implement these owner decisions.

Baseline inspected: gitpm dev commit `27ceb64` (2026-10-24). Recheck with `git rev-parse HEAD`
and the paths below when implementing; these observations are not environment
limitations or workarounds:

- `openspec/specs/install-uninstall/spec.md`, "Install an explicit ref or a branch",
  and `crates/gitpm/src/model.rs` (`Mode`): `--dev` is origin-default-branch following;
  state Mode has Release, Pinned and Branch. The old channel problem is resolved.
- `openspec/specs/dist/spec.md`, "Deploys copy the committed tree": managed
  deployments select the committed manifest `dist` folder (currently defaulting to
  `<path>/dist`) from Git objects, not live files. A link is an
  explicit exception, not an alternate implementation of that copy operation.
- `openspec/specs/workspace-layout/spec.md`, "Installed package record", and
  `crates/gitpm/src/model.rs` (`Installed`): current/previous revisions assume an exact
  commit. Uncommitted bytes cannot truthfully be described by that revision.
- `openspec/specs/transactions-locking/spec.md`: staged paths, journal, state-last
  commit, roll-forward recovery, real managed parents. A leaf symlink must not
  relax the managed-parent checks.
- `crates/gitpm/src/check.rs` (`CheckRow`), `crates/gitpm/src/workspace.rs`
  (`UnchangedReason`) and `crates/gitpm-ffi/src/lib.rs` (`Mode`): mode and update/check reasons already cross
  public boundaries, but none represents a live link. Adding a CLI-only warning
  would not satisfy the owner decision on structured library/FFI reporting.

Prior owner decisions retained: one fixed workspace layout (2026-10-18) and
packages as folders in multi-package repositories selected through `gitpm.json`
(2026-10-16). Link keeps
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

Use a distinct `link LOCAL_PACKAGE_PATH` command and matching Manager/FFI
method, rather than `install --dev`: the latter already names remote branch
following. The library validates and resolves the local package and returns all
domain errors; CLI only parses paths and prints results. No remote URL is a link
target. A selected package folder inside a multi-package repository is supported.

Dan selected a unified publishing-folder model for managed install and link:
use `plugin.json.dist` when configured, otherwise `"dist"`. An explicit `"."`
selects the package root. For a multi-package repository, resolve the path
relative to the selected package folder. Reuse the existing path validation and
`buildCommand` / `buildRequired` metadata. gitpm never runs builds or watchers.
Existing packages keep their current layout; no managed default or manifest
requirement changes are needed.

Resolve an existing directory to a canonical absolute target; reject paths that
would create a workspace/self-reference cycle. Validate identity and manifest (`invalid_manifest`)
before staging anything. Record an absolute path so CLI cwd changes cannot retarget
a link. A missing local link output fails with `workspace`; the managed-revision
missing-output rule remains `invalid_manifest`. Neither case instructs gitpm to run a build.
No build script or hook from the package is executed. Existing source validation
rules still govern any optional origin used for release discovery.

For already-recorded link IDs, validated relink refreshes the manifest snapshot
and may select a new output folder. An unexpected deployed leaf is a conflict.
Managed installations must first be explicitly uninstalled using the existing
removal plan and lost-work checks, then linked; no update target converts a link.
Returning to managed installation likewise requires explicit uninstall followed
by install. External files remain outside every removal plan.

## Fork preparation and explicit live installation

The fork model inspected on 2026-10-24 only prepares an author repository:
`fork ID --ns NAMESPACE --into DIR` preserves history and inherited tags, writes
manifest provenance and a replacement offer, and leaves workspace state and dist
unchanged. It creates no release and performs no install or replacement.
The removed fork options `--replace`, `--version` and `--force` have no role in
the live workflow.

For a single-package repository, the link entry point gives this flow:

```sh
gitpm fork acme/hello --ns me --into hello-fork
# Prepare the manifest-selected output yourself if it does not exist.
gitpm link ./hello-fork
```

The second command explicitly installs `me/hello` in link mode. No preliminary
commit or new release is required; edits in the selected installation folder
become visible immediately. For a multi-package fork, pass the selected package
folder to link; siblings are not implicitly installed. The original remains
installed and its replacement binding is unchanged.

Manifest `forkOf` supplies validated discovery provenance, stored as described
below. Manifest `replaces` remains an offer and must not create a replacement
binding during link. Store the validated offer as top-level `replaces` in the
link record, using the same format as managed records. The existing explicit
`replace ORIGINAL --with FORK` reads it without a new API, subject to the
existing enabled-fork, compatibility and dependency rules. For example, after
the link above, `gitpm replace acme/hello --with me/hello` makes consumers of
`acme/hello` use the live fork deployment. The binding is created only by that
explicit step.

## State and revisions (original question 3)

Dan requires an explicit link marker and delegates the concrete format to this
design. Use a discriminated package record: managed packages retain their
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
`dependencies`, `metadata`; `source` when available. For a fork, also record
`mode.link.discovery.forkOf`: a validated snapshot using the existing manifest
`forkOf` shape (`id`, `version`, `commit`, optional `source`) and validation.
Use it for original-upstream discovery until a validated relink refreshes it;
external manifest edits must not retarget discovery. This descriptor is provenance,
not a managed replacement binding. Absent at the record top level: `current`,
`previous`, `clone`, `path`, `catalog`, `forkOf`. Optional top-level `replaces`
is the validated manifest offer, in the existing managed-record shape
(`id`, `version`); it is not a replacement binding. The manifest identity,
version, dependencies, metadata and replacement offer are a
validated snapshot at link time; file content stays live. They do not silently
change the dependency graph when the author edits the manifest. An explicit
relink refreshes that snapshot after validation and dependency checks. A displayed
version is a declared version, not a reproducibility guarantee. Source identifies
the release-discovery repository; it is not proof of workspace ownership. When
there is no usable origin, return an availability problem instead of inventing a
latest release. A local repository path may be a valid source under existing rules.

The representation and public API changes are **BREAKING**, but follow the
existing state format policy: the inspected baseline uses format 1, and schema
changes stay in format 1 before the first public release without backward
compatibility. Later, a bump belongs to the release milestone, at most once per
release when the previous public version would misread the new state. Refuse
foreign formats unchanged and document recreation, without a migration reader. Forgotten link records retain the link descriptor,
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
permit its later removal as if no external user relied on it: refuse
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

See `tasks.md` for the implementation plan. Use
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
- Prepare a fork from a hermetic local installed source; assert that fork alone
  preserves workspace state, dist and release tags. Explicitly link one selected
  fork package without a new release; assert live deployment, no sibling installs
  and no replacement binding. Verify the validated top-level `replaces` offer
  in state/FFI, then explicitly replace the original with the enabled linked fork
  using the existing API; assert consumers use the live fork and existing
  replacement/dependency checks apply. Verify that external manifest edits do
  not change the recorded offer until validated relink. Verify
  `mode.link.discovery.forkOf` in state/FFI,
  own-origin versus original-upstream availability, unchanged discovery after
  external manifest edits, and snapshot refresh only after validated relink.
- Keep managed install/update/deploy/lost-work regression coverage; document the
  state representation and command alongside the implementation.
