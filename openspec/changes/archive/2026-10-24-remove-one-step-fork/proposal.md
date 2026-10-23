# Separate fork preparation from release and installation

## Why
Fork must prepare an editable repository without attempting a release with an inherited tag or installing and replacing sibling packages implicitly.

## What Changes
- Remove the composed fork/release/install/replace API, resume and outcome records, and the CLI fork options `--replace`, `--version` and `--force`.
- Keep repository history, inherited tags, namespace rewriting and origin metadata when forking; leave installed packages, replacement bindings, workspace state and dist unchanged.
- Present explicit `fork` → `commit` → `install` → `replace` steps in CLI output and documentation.
- Keep explicit install and replace behavior and the committed dist installation model.
- Reconcile the pending source-fork change's CLI validation with the removed options.

## Impact
Breaking removal of composed library and FFI methods and fork-only options. Ordinary fork, commit, install, replace and switch-back remain separate operations. Existing release tags continue to conflict with an explicit commit of the same tag.
