## Design
Expose `fork_rewrite(directory)` as a workspace-independent author operation and `forkRewrite(directory)` through FFI. The CLI parses `fork-rewrite DIR` and presents the returned dependency rewrites. No workspace is opened or mutated.

Read current repository package manifests and build the original UUID to fork UUID/version mapping from `forkOf`. Reject missing fork provenance and ambiguous original UUIDs. Rewrite only dependencies whose original is in that mapping, preserving compatible requirements and replacing incompatible requirements with `^VERSION`. Reject a dependency on both the original and its fork. Validate and serialize all changed manifests before writing any, preserving metadata and unrelated author edits. The operation does not stage, commit, tag, install or replace; repeated calls return an empty list.

This edits author files, following the existing authoring model rather than workspace transactions. Validation failures write nothing. Write errors trigger best-effort restoration of the byte snapshots, as in existing author release operations. A process crash can leave a partial rewrite, and rerunning completes the remaining files. No workspace journal or workspace lock is involved.

Fork keeps its identity commit and returns packages but no rewrite list. Next steps show the optional explicit rewrite before commit.
